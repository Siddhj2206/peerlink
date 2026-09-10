import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { PartyClient } from "./client";
import type { MediaOpened, PartyEvent, PartyState } from "./types";

/**
 * The desktop implementation, built on Tauri's `invoke` and `listen`.
 *
 * Command names and event names here must match the handlers registered in
 * `src-tauri`.
 */
export class TauriPartyClient implements PartyClient {
  pickMediaFile(): Promise<string | null> {
    return invoke<string | null>("pick_media_file");
  }

  openMedia(path: string): Promise<MediaOpened> {
    return invoke<MediaOpened>("open_media", { path });
  }

  closeMedia(): Promise<void> {
    return invoke<void>("close_media");
  }

  getState(): Promise<PartyState> {
    return invoke<PartyState>("get_state");
  }

  subscribe(handler: (event: PartyEvent) => void): () => void {
    // `listen` is async, so unsubscription has to wait for registration.
    const pending: Promise<UnlistenFn>[] = [
      listen<MediaOpened>("media-opened", (event) =>
        handler({ type: "media-opened", media: event.payload }),
      ),
      listen<string>("error", (event) =>
        handler({ type: "error", message: event.payload }),
      ),
    ];
    let unsubscribed = false;
    return () => {
      if (unsubscribed) return;
      unsubscribed = true;
      void Promise.all(pending).then((unlisten) =>
        unlisten.forEach((off) => off()),
      );
    };
  }
}
