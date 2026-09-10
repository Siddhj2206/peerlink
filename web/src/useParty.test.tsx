import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { PartyClient, PartyEvent, PartyState } from "./party-client";
import { useParty } from "./useParty";

/** A `PartyClient` whose events the test can fire by hand. */
function fakeClient(overrides: Partial<PartyClient> = {}) {
  let handler: ((event: PartyEvent) => void) | null = null;

  const client: PartyClient = {
    pickMediaFile: vi.fn(async () => "/movies/one.mp4"),
    openMedia: vi.fn(async (path: string) => {
      const media = {
        url: "http://127.0.0.1:1234/media/token",
        name: path.split("/").pop() ?? "media",
      };
      // The real backend broadcasts this as part of opening.
      handler?.({ type: "media-opened", media });
      return media;
    }),
    closeMedia: vi.fn(async () => undefined),
    getState: vi.fn(async (): Promise<PartyState> => ({ media: null })),
    subscribe: vi.fn((next: (event: PartyEvent) => void) => {
      handler = next;
      return () => {
        handler = null;
      };
    }),
    ...overrides,
  };

  return {
    client,
    emit: (event: PartyEvent) => handler?.(event),
  };
}

describe("useParty", () => {
  it("starts idle and loads state from the client", async () => {
    const { client } = fakeClient();
    const { result } = renderHook(() => useParty(client));

    await waitFor(() => expect(client.getState).toHaveBeenCalledOnce());
    expect(result.current.state.media).toBeNull();
  });

  it("picks and opens a file, updating state from the broadcast event", async () => {
    const { client } = fakeClient();
    const { result } = renderHook(() => useParty(client));
    await waitFor(() => expect(client.subscribe).toHaveBeenCalled());

    await act(async () => {
      await result.current.openFile();
    });

    expect(client.pickMediaFile).toHaveBeenCalledOnce();
    expect(client.openMedia).toHaveBeenCalledWith("/movies/one.mp4");
    expect(result.current.state.media).toEqual({
      url: "http://127.0.0.1:1234/media/token",
      name: "one.mp4",
    });
    expect(result.current.error).toBeNull();
  });

  it("does nothing when the picker is cancelled", async () => {
    const { client } = fakeClient({ pickMediaFile: vi.fn(async () => null) });
    const { result } = renderHook(() => useParty(client));

    await act(async () => {
      await result.current.openFile();
    });

    expect(client.openMedia).not.toHaveBeenCalled();
    expect(result.current.state.media).toBeNull();
  });

  it("surfaces an error when opening fails", async () => {
    const { client } = fakeClient({
      openMedia: vi.fn(async () => {
        throw "could not open /movies/one.mp4";
      }),
    });
    const { result } = renderHook(() => useParty(client));

    await act(async () => {
      await result.current.openFile();
    });

    expect(result.current.error).toBe("could not open /movies/one.mp4");
    expect(result.current.state.media).toBeNull();
  });

  it("follows media-opened events from the backend", async () => {
    const { client, emit } = fakeClient();
    const { result } = renderHook(() => useParty(client));
    await waitFor(() => expect(client.getState).toHaveBeenCalled());

    act(() => {
      emit({
        type: "media-opened",
        media: { url: "http://127.0.0.1:1/media/x", name: "x.mp4" },
      });
    });

    expect(result.current.state.media?.name).toBe("x.mp4");
  });

  it("surfaces backend errors and can dismiss them", async () => {
    const { client, emit } = fakeClient();
    const { result } = renderHook(() => useParty(client));
    await waitFor(() => expect(client.getState).toHaveBeenCalled());

    act(() => {
      emit({ type: "error", message: "no such file" });
    });
    expect(result.current.error).toBe("no such file");

    act(() => {
      result.current.dismissError();
    });
    expect(result.current.error).toBeNull();
  });

  it("closes the media", async () => {
    const { client } = fakeClient();
    const { result } = renderHook(() => useParty(client));
    await waitFor(() => expect(client.subscribe).toHaveBeenCalled());

    await act(async () => {
      await result.current.openFile();
    });
    await act(async () => {
      await result.current.closeMedia();
    });

    expect(client.closeMedia).toHaveBeenCalledOnce();
    expect(result.current.state.media).toBeNull();
  });
});
