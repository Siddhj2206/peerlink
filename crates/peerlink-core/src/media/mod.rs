//! Getting media bytes to the webview.
//!
//! The webview plays a plain `<video>` element, and on Linux WebKitGTK cannot
//! play media from a custom URI scheme or `asset://`. A loopback HTTP server is
//! therefore the only path verified to work across desktop platforms, so the
//! `<video src>` points here and the browser speaks ordinary byte-range HTTP.
//!
//! The server does not care where the bytes live: everything sits behind
//! [`MediaSource`], with [`LocalFile`] reading the Host's own disk today and a
//! remote adapter to follow.

mod server;

pub use server::{CHUNK_SIZE, LocalFile, MediaServer, MediaSource};
