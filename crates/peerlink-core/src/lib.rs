//! Pure domain for PeerLink.
//!
//! This crate holds behaviour that has no dependency on a windowing toolkit, a
//! webview, or Tauri: room code generation, ticket serialisation, and the room
//! lifecycle state machine. It is deliberately small and unit-tested, so the
//! logic can be exercised without a window or a display server.
//!
//! The public surface is re-exported at the crate root; the `domain` module is
//! public only so the pieces can be addressed individually.

pub mod domain;

pub use domain::{CODE_WORDS, PartyTicket, RoomState, generate_room_code, words_to_topic_id};
