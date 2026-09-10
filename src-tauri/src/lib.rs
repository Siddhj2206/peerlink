//! PeerLink's Tauri shell.
//!
//! The shell is deliberately thin. It owns the managed media-server state and
//! registers the command handlers; all behaviour lives in `peerlink-core`.

mod model;
mod state;

pub mod commands;
pub mod events;
pub use model::{MediaOpened, PartyState};
pub use state::MediaState;

/// Register plugins, managed state, and command handlers on `builder`.
///
/// Split out from [`run`] so the integration test can build a mock app that
/// exposes the exact same command and event surface as the real one.
pub fn configure<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder
        .plugin(tauri_plugin_dialog::init())
        .manage(MediaState::default())
        .invoke_handler(tauri::generate_handler![
            commands::pick_media_file,
            commands::open_media,
            commands::close_media,
            commands::get_state,
        ])
}

/// Run the desktop app.
pub fn run() {
    configure(tauri::Builder::default())
        .run(tauri::generate_context!())
        .expect("error while running PeerLink");
}
