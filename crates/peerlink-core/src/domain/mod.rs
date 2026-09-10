//! The pure domain: room codes, tickets, and the room lifecycle.

mod room;
mod room_code;
mod ticket;

pub use room::RoomState;
pub use room_code::{CODE_WORDS, generate_room_code, words_to_topic_id};
pub use ticket::PartyTicket;
