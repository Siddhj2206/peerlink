import type { PartyClient } from "./party-client";
import { useParty } from "./useParty";
import { Home } from "./views/Home";
import { Party } from "./views/Party";

/**
 * Chooses the screen from session state. It owns no domain state itself; the
 * backend is authoritative and this just renders what `useParty` reports.
 */
export default function App({ client }: { client: PartyClient }) {
  const { state, error, busy, dismissError, openFile, closeMedia } =
    useParty(client);

  return (
    <div className="app">
      {error !== null && (
        <div role="alert" className="error">
          <span>{error}</span>
          <button type="button" onClick={dismissError}>
            Dismiss
          </button>
        </div>
      )}
      {state.media !== null ? (
        <Party media={state.media} onClose={closeMedia} />
      ) : (
        <Home busy={busy} onOpen={openFile} />
      )}
    </div>
  );
}
