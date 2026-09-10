import type { MediaOpened, PartyEvent, PartyState } from "./types";

/**
 * The seam every view talks to.
 *
 * Views never import Tauri directly, so a future browser client can implement
 * this interface without any view code changing.
 */
export interface PartyClient {
  /** Open the native file picker; resolves to `null` if the user cancels. */
  pickMediaFile(): Promise<string | null>;

  /** Serve the file at `path` on the loopback server and return its URL. */
  openMedia(path: string): Promise<MediaOpened>;

  /** Stop serving the current file. */
  closeMedia(): Promise<void>;

  /** Fetch the current session state, for first render or a reconnect. */
  getState(): Promise<PartyState>;

  /** Subscribe to backend events; the returned function unsubscribes. */
  subscribe(handler: (event: PartyEvent) => void): () => void;
}
