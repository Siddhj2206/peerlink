//! PeerLink's UI-free core.
//!
//! This crate holds behaviour that has no dependency on a windowing toolkit, a
//! webview, or Tauri: the pure domain (room code generation, ticket
//! serialisation, and the room lifecycle state machine) and the loopback media
//! server that feeds the webview's `<video>` element. It is deliberately small
//! and testable without a window or a display server.
//!
//! The public surface is re-exported at the crate root; the modules are public
//! so the pieces can also be addressed individually.

pub mod domain;
pub mod media;

pub use domain::{CODE_WORDS, PartyTicket, RoomState, generate_room_code, words_to_topic_id};
pub use media::{LocalFile, MediaServer, MediaSource};
