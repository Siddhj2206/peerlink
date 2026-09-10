//! The serialised shapes shared with the webview.
//!
//! The frontend's TypeScript mirrors these types, so any change here has to be
//! reflected in `web/src/party-client/types.ts`.

use serde::{Deserialize, Serialize};

/// A file the loopback server is currently serving to the `<video>` element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaOpened {
    /// The `<video src>` URL, including the server's per-launch token.
    pub url: String,
    /// The file's base name, for display.
    pub name: String,
}

/// The snapshot returned by the `get_state` command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartyState {
    /// The media currently being served, if any.
    pub media: Option<MediaOpened>,
}
