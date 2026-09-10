import { useCallback, useEffect, useState } from "react";

import type { PartyClient, PartyState } from "./party-client";

const EMPTY: PartyState = { media: null };

export interface UseParty {
  /** The session state the views render. */
  state: PartyState;
  /** The last error, shown until dismissed. */
  error: string | null;
  /** Whether a pick/open round-trip is in flight. */
  busy: boolean;
  /** Clear the current error. */
  dismissError(): void;
  /** Pick an MP4 and start serving it locally. */
  openFile(): Promise<void>;
  /** Stop serving the current file. */
  closeMedia(): Promise<void>;
}

/**
 * Holds the session state and keeps it in step with backend events.
 *
 * It depends only on `PartyClient`, so the same hook drives the desktop app
 * today and a browser client later. The backend is authoritative: the open
 * flow changes state by broadcasting `media-opened`, not by its return value,
 * so this hook has a single update path.
 */
export function useParty(client: PartyClient): UseParty {
  const [state, setState] = useState<PartyState>(EMPTY);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let active = true;
    client.getState().then(
      (next) => {
        if (active) setState(next);
      },
      (cause) => {
        if (active) setError(message(cause));
      },
    );
    const unsubscribe = client.subscribe((event) => {
      if (event.type === "media-opened") {
        setState((prev) => ({ ...prev, media: event.media }));
      } else if (event.type === "error") {
        setError(event.message);
      }
    });
    return () => {
      active = false;
      unsubscribe();
    };
  }, [client]);

  // One place that clears the previous error, runs the action, and reports a
  // failure — instead of the same try/catch in every handler.
  const guard = useCallback(async (action: () => Promise<unknown>) => {
    setError(null);
    try {
      await action();
    } catch (cause) {
      setError(message(cause));
    }
  }, []);

  const openFile = useCallback(async () => {
    setBusy(true);
    await guard(async () => {
      const path = await client.pickMediaFile();
      if (path === null) return;
      await client.openMedia(path);
    });
    setBusy(false);
  }, [client, guard]);

  const closeMedia = useCallback(
    () =>
      guard(async () => {
        await client.closeMedia();
        setState((prev) => ({ ...prev, media: null }));
      }),
    [client, guard],
  );

  return {
    state,
    error,
    busy,
    dismissError: () => setError(null),
    openFile,
    closeMedia,
  };
}

function message(cause: unknown): string {
  if (typeof cause === "string") return cause;
  if (cause instanceof Error) return cause.message;
  return String(cause);
}
