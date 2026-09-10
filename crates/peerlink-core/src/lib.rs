//! Pure domain for PeerLink.
//!
//! This crate holds behaviour that has no dependency on a windowing toolkit, a
//! webview, or Tauri: room code generation, ticket serialisation, and the room
//! lifecycle state machine. It is deliberately small and heavily unit-tested,
//! so the same logic can back the parked `peerlink-egui` prototype and the
//! Tauri rewrite.

pub mod domain;

pub use domain::{CODE_WORDS, PartyTicket, RoomState, generate_room_code, words_to_topic_id};
