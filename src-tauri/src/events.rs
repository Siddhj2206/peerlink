//! The events the shell pushes to the webview.
//!
//! The frontend's `TauriPartyClient` listens for exactly these names, so they
//! live in one place rather than as loose strings in the command handlers.

use tauri::{AppHandle, Emitter, Runtime};

use crate::model::MediaOpened;

/// Emitted after a file starts being served.
pub const MEDIA_OPENED: &str = "media-opened";

/// Emitted when the media flow fails, so a listener sees it even if it missed
/// the command's rejected promise.
pub const ERROR: &str = "error";

/// Broadcast that a file started being served.
pub fn media_opened<R: Runtime>(app: &AppHandle<R>, media: &MediaOpened) {
    let _ = app.emit(MEDIA_OPENED, media);
}

/// Broadcast that the media flow failed.
pub fn error<R: Runtime>(app: &AppHandle<R>, message: &str) {
    let _ = app.emit(ERROR, message);
}
