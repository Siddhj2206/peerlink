//! The Tauri command surface.
//!
//! These handlers are a thin adapter: they translate between the webview and
//! `peerlink-core`, which owns the behaviour. The same handlers are registered
//! by the real app and by the integration test, so the test crosses the same
//! seam the frontend does.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Runtime, State};
use tauri_plugin_dialog::{DialogExt, FilePath};

use peerlink_core::MediaServer;

use crate::events;
use crate::model::{MediaOpened, PartyState};
use crate::state::MediaState;

/// Ask the user for an MP4 with the native dialog.
///
/// Resolves to `None` when the picker is cancelled. The dialog plugin's
/// callback API is used rather than its blocking one, which must not run on the
/// main thread.
#[tauri::command]
pub async fn pick_media_file<R: Runtime>(app: AppHandle<R>) -> Option<String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("MP4 video", &["mp4", "m4v"])
        .pick_file(move |path| {
            let _ = tx.send(path);
        });
    picked_path(rx.await.ok().flatten())
}

/// Turn the dialog's result into a filesystem path string.
///
/// Split from the command so the non-dialog half has a unit test; the picker
/// itself cannot be driven headlessly.
fn picked_path(path: Option<FilePath>) -> Option<String> {
    let path = path?.into_path().ok()?;
    Some(path.to_string_lossy().into_owned())
}

/// Serve a local file on the loopback media server and return its `<video>`
/// URL. Emits [`events::MEDIA_OPENED`] on success and [`events::ERROR`] on
/// failure.
#[tauri::command]
pub async fn open_media<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, MediaState>,
    path: String,
) -> Result<MediaOpened, String> {
    match serve(&PathBuf::from(path)).await {
        Ok((server, media)) => {
            state.set(server, media.name.clone());
            events::media_opened(&app, &media);
            Ok(media)
        }
        Err(message) => {
            events::error(&app, &message);
            Err(message)
        }
    }
}

/// Stop serving the current file.
#[tauri::command]
pub fn close_media(state: State<'_, MediaState>) {
    state.clear();
}

/// The current session snapshot, for first render or a reconnect.
#[tauri::command]
pub fn get_state(state: State<'_, MediaState>) -> PartyState {
    PartyState {
        media: state.snapshot(),
    }
}

/// Open `path` behind a loopback server.
async fn serve(path: &Path) -> Result<(MediaServer, MediaOpened), String> {
    let server = MediaServer::serve_file(path)
        .await
        .map_err(|cause| format!("could not open {}: {cause}", path.display()))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("video.mp4")
        .to_owned();
    let media = MediaOpened {
        url: server.url(),
        name,
    };
    Ok((server, media))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picked_path_converts_a_chosen_file() {
        let chosen = FilePath::from(PathBuf::from("/movies/one.mp4"));
        assert_eq!(
            picked_path(Some(chosen)).as_deref(),
            Some("/movies/one.mp4")
        );
    }

    #[test]
    fn picked_path_is_none_when_the_picker_is_cancelled() {
        assert_eq!(picked_path(None), None);
    }
}
