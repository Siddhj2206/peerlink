//! The one piece of mutable state the Tauri layer owns: which file is being
//! served to the webview, and the loopback server doing the serving.
//!
//! There is no domain state here — the core owns that. The loopback server
//! belongs to `peerlink-core` (and will be owned by the `Party` aggregate once
//! it exists); until then the shell keeps the running server alive here so it
//! outlives a single command call.

use std::sync::Mutex;

use peerlink_core::MediaServer;

use crate::model::MediaOpened;

/// A file and the server currently answering `<video>` range requests for it.
struct Serving {
    server: MediaServer,
    name: String,
}

/// The app-managed state behind `open_media`, `close_media`, and `get_state`.
#[derive(Default)]
pub struct MediaState {
    inner: Mutex<Option<Serving>>,
}

impl MediaState {
    /// Serve `server`'s file, stopping whatever was served before.
    pub fn set(&self, server: MediaServer, name: String) {
        *self.lock() = Some(Serving { server, name });
    }

    /// Stop serving the current file. Dropping [`MediaServer`] aborts its task.
    pub fn clear(&self) {
        *self.lock() = None;
    }

    /// The current media as the webview sees it.
    ///
    /// The URL is read from the live server, so state holds only the name and
    /// the handle that keeps the server alive — never a stale copy of the URL.
    pub fn snapshot(&self) -> Option<MediaOpened> {
        self.lock().as_ref().map(|serving| MediaOpened {
            url: serving.server.url(),
            name: serving.name.clone(),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Serving>> {
        self.inner.lock().expect("media state mutex poisoned")
    }
}
