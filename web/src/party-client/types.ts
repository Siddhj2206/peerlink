// The data contract between the views and whichever backend answers them.
//
// These types mirror the serialised Rust payloads. A browser client can be
// added later without touching the views, as long as it speaks this shape.

/** A media file the loopback server is currently serving. */
export interface MediaOpened {
  /** `<video src>` URL on the loopback server, including its access token. */
  url: string;
  /** The picked file's base name, for display. */
  name: string;
}

/** Everything the current session knows, as fetched by `getState`. */
export interface PartyState {
  media: MediaOpened | null;
}

/** Pushed from the backend as the session changes. */
export type PartyEvent =
  | { type: "media-opened"; media: MediaOpened }
  | { type: "error"; message: string };
