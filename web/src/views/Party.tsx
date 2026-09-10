import { useState } from "react";

import type { MediaOpened } from "../party-client";

interface PartyProps {
  media: MediaOpened;
  onClose(): void;
}

/**
 * The player screen.
 *
 * `media.url` points at the loopback media server, not a Tauri asset URL:
 * WebKitGTK cannot play media from a custom scheme, but it plays byte-range
 * HTTP correctly, which is also what makes seeking work.
 */
export function Party({ media, onClose }: PartyProps) {
  const [playbackError, setPlaybackError] = useState<string | null>(null);

  return (
    <main className="party">
      <header>
        <h1 title={media.name}>{media.name}</h1>
        <button type="button" onClick={onClose}>
          Close
        </button>
      </header>
      {playbackError !== null && (
        <p role="alert" className="playback-error">
          {playbackError}
        </p>
      )}
      <video
        src={media.url}
        controls
        autoPlay
        playsInline
        onError={(event) => setPlaybackError(describePlaybackError(event.currentTarget.error))}
      />
    </main>
  );
}

/** Turn a `<video>` failure into a message a person can act on. */
function describePlaybackError(error: MediaError | null): string {
  switch (error?.code) {
    case MediaError.MEDIA_ERR_SRC_NOT_SUPPORTED:
      return "This file's format or codec is not supported.";
    case MediaError.MEDIA_ERR_DECODE:
      return "This video could not be decoded.";
    case MediaError.MEDIA_ERR_NETWORK:
      return "The video stopped because the connection failed.";
    case MediaError.MEDIA_ERR_ABORTED:
      return "Playback was aborted.";
    default:
      return "This video could not be played.";
  }
}
