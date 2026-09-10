//! Integration check for the media command and event surface.
//!
//! It builds a mock Tauri app registering the *same* handlers the real app
//! does, then drives the open/close/state commands the way the webview would:
//! invoke them, observe the emitted events, and fetch the returned URL over
//! real HTTP. (`pick_media_file` opens a native dialog, so it cannot be driven
//! headlessly; it is exercised by the real app.)

use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

use peerlink_lib::events::{ERROR, MEDIA_OPENED};
use peerlink_lib::{MediaOpened, PartyState, configure};
use tauri::test::{MockRuntime, get_ipc_response, mock_builder};
use tauri::webview::InvokeRequest;
use tauri::{Listener, WebviewWindow, WebviewWindowBuilder};
use tempfile::NamedTempFile;

/// A temporary MP4 whose bytes differ at almost every offset, so a wrong range
/// is visible.
fn temp_mp4(len: usize) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("create temp file");
    let bytes: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
    file.write_all(&bytes).expect("write temp file");
    file.flush().expect("flush temp file");
    file
}

/// A mock app + webview exposing the real command surface.
fn mock_webview() -> (tauri::App<MockRuntime>, WebviewWindow<MockRuntime>) {
    let app = configure(mock_builder())
        // The real context, so application commands are permitted by the ACL
        // exactly as they are in the shipping app.
        .build(tauri::generate_context!())
        .expect("build mock app");
    let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("build mock webview");
    (app, webview)
}

/// Drive one command exactly as the IPC layer would.
fn invoke<T: serde::de::DeserializeOwned>(
    webview: &WebviewWindow<MockRuntime>,
    cmd: &str,
    body: serde_json::Value,
) -> Result<T, serde_json::Value> {
    get_ipc_response(
        webview,
        InvokeRequest {
            cmd: cmd.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            // The local webview origin; anything else is treated as remote and
            // re-checked against the ACL.
            url: if cfg!(any(windows, target_os = "android")) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            }
            .parse()
            .unwrap(),
            body: tauri::ipc::InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        },
    )
    .map(|body| {
        body.deserialize::<T>()
            .expect("command returned an unexpected shape")
    })
}

/// Collect event payloads into a shared list for assertions.
fn collect<T: serde::de::DeserializeOwned + Send + 'static>(
    app: &tauri::App<MockRuntime>,
    event: &'static str,
) -> Arc<Mutex<Vec<T>>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    app.listen(event, move |event| {
        let payload = event.payload();
        let value = serde_json::from_str(payload).expect("event payload is JSON");
        sink.lock().unwrap().push(value);
    });
    seen
}

#[test]
fn opening_a_file_serves_it_over_loopback_and_emits_media_opened() {
    let (app, webview) = mock_webview();
    let opened = collect::<MediaOpened>(&app, MEDIA_OPENED);
    let file = temp_mp4(200 * 1024);
    let path = file.path().to_string_lossy().into_owned();

    let media: MediaOpened = invoke(&webview, "open_media", serde_json::json!({ "path": path }))
        .expect("open_media should succeed");

    // The returned URL is the loopback server, not a Tauri asset URL.
    assert!(media.url.starts_with("http://127.0.0.1:"), "{}", media.url);
    assert_eq!(
        media.name,
        Path::new(&path).file_name().unwrap().to_string_lossy()
    );

    // The same media was pushed as an event.
    assert_eq!(*opened.lock().unwrap(), vec![media.clone()]);

    // And the server really answers the `<video>` HEAD/Range contract.
    let head = tauri::async_runtime::block_on(async {
        reqwest::Client::new().head(&media.url).send().await
    })
    .expect("HEAD the media URL");
    assert_eq!(head.status(), reqwest::StatusCode::OK);
    assert_eq!(head.headers()[reqwest::header::ACCEPT_RANGES], "bytes");
    assert_eq!(head.headers()[reqwest::header::CONTENT_LENGTH], "204800");

    let (status, body_len) = tauri::async_runtime::block_on(async {
        let response = reqwest::Client::new()
            .get(&media.url)
            .header(reqwest::header::RANGE, "bytes=10-19")
            .send()
            .await?;
        let status = response.status();
        let body = response.bytes().await?;
        Ok::<_, reqwest::Error>((status, body.len()))
    })
    .expect("Range GET the media URL");
    assert_eq!(status, reqwest::StatusCode::PARTIAL_CONTENT);
    assert_eq!(body_len, 10);
}

#[test]
fn get_state_reports_the_open_file_and_close_media_stops_serving() {
    let (_app, webview) = mock_webview();
    let file = temp_mp4(4096);
    let path = file.path().to_string_lossy().into_owned();

    let idle: PartyState =
        invoke(&webview, "get_state", serde_json::json!({})).expect("get_state before open");
    assert_eq!(idle.media, None);

    let media: MediaOpened =
        invoke(&webview, "open_media", serde_json::json!({ "path": path })).expect("open_media");

    let opened: PartyState =
        invoke(&webview, "get_state", serde_json::json!({})).expect("get_state after open");
    assert_eq!(opened.media, Some(media.clone()));

    invoke::<()>(&webview, "close_media", serde_json::json!({})).expect("close_media");

    let closed: PartyState =
        invoke(&webview, "get_state", serde_json::json!({})).expect("get_state after close");
    assert_eq!(closed.media, None);
}

#[test]
fn opening_a_missing_file_reports_and_emits_an_error() {
    let (app, webview) = mock_webview();
    let errors = collect::<String>(&app, ERROR);
    let missing = "/does/not/exist/movie.mp4";

    let result: Result<MediaOpened, serde_json::Value> = invoke(
        &webview,
        "open_media",
        serde_json::json!({ "path": missing }),
    );

    let error = result.expect_err("open_media should reject a missing file");
    let message = error.as_str().expect("error payload is a string");
    assert!(message.contains(missing), "{message}");
    assert_eq!(*errors.lock().unwrap(), vec![message.to_owned()]);
}
