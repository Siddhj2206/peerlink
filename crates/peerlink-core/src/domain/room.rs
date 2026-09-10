//! The room lifecycle state machine.
//!
//! `RoomState` is the single source of truth for which screen a UI should show
//! and what a Host or Joiner is currently allowed to do. Every transition
//! consumes `self` and returns the next state, so an illegal transition cannot
//! be expressed — there is no `.unwrap()` for a caller to forget.

use iroh_docs::NamespaceId;
use iroh_gossip::TopicId;

/// Where the app is in the create/join lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RoomState {
    /// Nothing in progress; the Home screen.
    #[default]
    Idle,
    /// This peer created the room and is the Host.
    Hosting {
        room_code: String,
        ticket_string: String,
        topic_id: TopicId,
        namespace_id: NamespaceId,
    },
    /// A ticket has been entered but not yet resolved.
    Joining { ticket_input: String },
    /// This peer joined a Host's room.
    Joined {
        room_code: String,
        topic_id: TopicId,
        namespace_id: NamespaceId,
    },
    /// The last action failed; the message is shown until dismissed.
    Error { message: String },
}

impl RoomState {
    /// Begin hosting a freshly created room.
    pub fn start_hosting(
        room_code: String,
        ticket_string: String,
        topic_id: TopicId,
        namespace_id: NamespaceId,
    ) -> Self {
        Self::Hosting {
            room_code,
            ticket_string,
            topic_id,
            namespace_id,
        }
    }

    /// Record that the user is entering a ticket.
    pub fn start_joining(ticket_input: String) -> Self {
        Self::Joining { ticket_input }
    }

    /// Resolve a pending join. No-op unless currently `Joining`.
    pub fn join(self, room_code: String, topic_id: TopicId, namespace_id: NamespaceId) -> Self {
        match self {
            Self::Joining { .. } => Self::Joined {
                room_code,
                topic_id,
                namespace_id,
            },
            other => other,
        }
    }

    /// Leave the room. `Idle` and `Error` are left untouched.
    pub fn leave(self) -> Self {
        match self {
            Self::Hosting { .. } | Self::Joined { .. } | Self::Joining { .. } => Self::Idle,
            other => other,
        }
    }

    /// Enter the error state.
    pub fn fail(message: String) -> Self {
        Self::Error { message }
    }

    /// Clear an error. No-op unless currently `Error`.
    pub fn dismiss_error(self) -> Self {
        match self {
            Self::Error { .. } => Self::Idle,
            other => other,
        }
    }

    pub fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    pub fn is_hosting(&self) -> bool {
        matches!(self, Self::Hosting { .. })
    }

    pub fn is_joining(&self) -> bool {
        matches!(self, Self::Joining { .. })
    }

    pub fn is_joined(&self) -> bool {
        matches!(self, Self::Joined { .. })
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error { .. })
    }

    /// The room code, once one is known.
    pub fn room_code(&self) -> Option<&str> {
        match self {
            Self::Hosting { room_code, .. } | Self::Joined { room_code, .. } => Some(room_code),
            _ => None,
        }
    }

    /// The gossip topic, once connected to a room.
    pub fn topic_id(&self) -> Option<TopicId> {
        match self {
            Self::Hosting { topic_id, .. } | Self::Joined { topic_id, .. } => Some(*topic_id),
            _ => None,
        }
    }

    /// The chat namespace, once connected to a room.
    pub fn namespace_id(&self) -> Option<NamespaceId> {
        match self {
            Self::Hosting { namespace_id, .. } | Self::Joined { namespace_id, .. } => {
                Some(*namespace_id)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn topic() -> TopicId {
        TopicId::from_bytes([0x11; 32])
    }

    fn namespace() -> NamespaceId {
        NamespaceId::from(&[0x22; 32])
    }

    fn hosting() -> RoomState {
        RoomState::start_hosting("code".into(), "ticket".into(), topic(), namespace())
    }

    #[test]
    fn default_is_idle() {
        assert!(RoomState::default().is_idle());
    }

    #[test]
    fn hosting_exposes_room_identity() {
        let state = hosting();
        assert!(state.is_hosting());
        assert_eq!(state.room_code(), Some("code"));
        assert_eq!(state.topic_id(), Some(topic()));
        assert_eq!(state.namespace_id(), Some(namespace()));
    }

    #[test]
    fn joining_hides_room_identity_until_resolved() {
        let state = RoomState::start_joining("ticket".into());
        assert!(state.is_joining());
        assert_eq!(state.room_code(), None);
        assert_eq!(state.topic_id(), None);
        assert_eq!(state.namespace_id(), None);
    }

    #[test]
    fn join_resolves_a_pending_join() {
        let state = RoomState::start_joining("ticket".into()).join("code".into(), topic(), namespace());
        assert!(state.is_joined());
        assert_eq!(state.room_code(), Some("code"));
        assert_eq!(state.topic_id(), Some(topic()));
        assert_eq!(state.namespace_id(), Some(namespace()));
    }

    #[test]
    fn join_outside_joining_is_a_no_op() {
        let state = RoomState::Idle.join("code".into(), topic(), namespace());
        assert!(state.is_idle());
    }

    #[test]
    fn leave_resets_hosting_and_joined_to_idle() {
        assert!(hosting().leave().is_idle());

        let joined = RoomState::start_joining("t".into()).join("c".into(), topic(), namespace());
        assert!(joined.leave().is_idle());
    }

    #[test]
    fn leave_is_a_no_op_from_idle_and_error() {
        assert!(RoomState::Idle.leave().is_idle());
        assert!(RoomState::fail("boom".into()).leave().is_error());
    }

    #[test]
    fn dismiss_error_clears_only_an_error() {
        assert!(RoomState::fail("boom".into()).dismiss_error().is_idle());
        assert!(hosting().dismiss_error().is_hosting());
    }

    #[test]
    fn error_hides_room_identity() {
        let state = RoomState::fail("boom".into());
        assert!(state.is_error());
        assert_eq!(state.room_code(), None);
        assert_eq!(state.topic_id(), None);
        assert_eq!(state.namespace_id(), None);
    }
}
