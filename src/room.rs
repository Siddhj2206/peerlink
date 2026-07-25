use iroh_docs::NamespaceId;
use iroh_gossip::TopicId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomState {
    Idle,
    Hosting {
        room_code: String,
        ticket_string: String,
        topic_id: TopicId,
        namespace_id: NamespaceId,
    },
    Joining {
        ticket_input: String,
    },
    Joined {
        room_code: String,
        topic_id: TopicId,
        namespace_id: NamespaceId,
    },
    Error {
        message: String,
    },
}

impl Default for RoomState {
    fn default() -> Self {
        Self::Idle
    }
}

impl RoomState {
    pub fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    pub fn is_hosting(&self) -> bool {
        matches!(self, Self::Hosting { .. })
    }

    pub fn is_joined(&self) -> bool {
        matches!(self, Self::Joined { .. })
    }

    pub fn is_joining(&self) -> bool {
        matches!(self, Self::Joining { .. })
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error { .. })
    }

    pub fn room_code(&self) -> Option<&str> {
        match self {
            Self::Hosting { room_code, .. } => Some(room_code),
            Self::Joined { room_code, .. } => Some(room_code),
            _ => None,
        }
    }

    pub fn topic_id(&self) -> Option<TopicId> {
        match self {
            Self::Hosting { topic_id, .. } => Some(*topic_id),
            Self::Joined { topic_id, .. } => Some(*topic_id),
            _ => None,
        }
    }

    pub fn namespace_id(&self) -> Option<NamespaceId> {
        match self {
            Self::Hosting { namespace_id, .. } => Some(*namespace_id),
            Self::Joined { namespace_id, .. } => Some(*namespace_id),
            _ => None,
        }
    }

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

    pub fn start_joining(ticket_input: String) -> Self {
        Self::Joining { ticket_input }
    }

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

    pub fn leave(self) -> Self {
        match self {
            Self::Hosting { .. } | Self::Joined { .. } | Self::Joining { .. } => Self::Idle,
            other => other,
        }
    }

    pub fn fail(message: String) -> Self {
        Self::Error { message }
    }

    pub fn dismiss_error(self) -> Self {
        match self {
            Self::Error { .. } => Self::Idle,
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_topic_id() -> TopicId {
        TopicId::from_bytes([
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
            0x1c, 0x1d, 0x1e, 0x1f,
        ])
    }

    fn test_namespace_id() -> NamespaceId {
        NamespaceId::from(&[
            0xff, 0xfe, 0xfd, 0xfc, 0xfb, 0xfa, 0xf9, 0xf8, 0xf7, 0xf6, 0xf5, 0xf4, 0xf3, 0xf2,
            0xf1, 0xf0, 0xef, 0xee, 0xed, 0xec, 0xeb, 0xea, 0xe9, 0xe8, 0xe7, 0xe6, 0xe5, 0xe4,
            0xe3, 0xe2, 0xe1, 0xe0,
        ])
    }

    #[test]
    fn test_idle_is_default() {
        let state = RoomState::default();
        assert!(state.is_idle());
        assert!(!state.is_hosting());
        assert!(!state.is_joined());
    }

    #[test]
    fn test_start_hosting_transition() {
        let topic_id = test_topic_id();
        let ns_id = test_namespace_id();
        let state = RoomState::start_hosting(
            "alpha bravo charlie delta echo foxtrot".into(),
            "partyabc123".into(),
            topic_id,
            ns_id,
        );
        assert!(state.is_hosting());
        assert_eq!(state.room_code(), Some("alpha bravo charlie delta echo foxtrot"));
        assert_eq!(state.topic_id(), Some(topic_id));
        assert_eq!(state.namespace_id(), Some(ns_id));
    }

    #[test]
    fn test_start_joining_transition() {
        let state = RoomState::start_joining("partyabc123".into());
        assert!(state.is_joining());
    }

    #[test]
    fn test_join_transition_from_joining() {
        let state = RoomState::start_joining("partyabc123".into());
        let state = state.join("room code".into(), test_topic_id(), test_namespace_id());
        assert!(state.is_joined());
        assert_eq!(state.room_code(), Some("room code"));
        assert_eq!(state.topic_id(), Some(test_topic_id()));
        assert_eq!(state.namespace_id(), Some(test_namespace_id()));
    }

    #[test]
    fn test_join_from_non_joining_is_noop() {
        let state = RoomState::Idle;
        let state = state.join("room code".into(), test_topic_id(), test_namespace_id());
        assert!(state.is_idle());
    }

    #[test]
    fn test_leave_hosting_returns_to_idle() {
        let state = RoomState::start_hosting(
            "room code".into(),
            "partyabc".into(),
            test_topic_id(),
            test_namespace_id(),
        );
        let state = state.leave();
        assert!(state.is_idle());
    }

    #[test]
    fn test_leave_joined_returns_to_idle() {
        let state = RoomState::start_joining("ticket".into());
        let state = state.join("room code".into(), test_topic_id(), test_namespace_id());
        let state = state.leave();
        assert!(state.is_idle());
    }

    #[test]
    fn test_leave_idle_is_noop() {
        let state = RoomState::Idle;
        let state = state.leave();
        assert!(state.is_idle());
    }

    #[test]
    fn test_error_transition() {
        let state = RoomState::fail("connection failed".into());
        assert!(state.is_error());
    }

    #[test]
    fn test_dismiss_error_returns_to_idle() {
        let state = RoomState::fail("connection failed".into());
        let state = state.dismiss_error();
        assert!(state.is_idle());
    }

    #[test]
    fn test_dismiss_error_from_idle_is_noop() {
        let state = RoomState::Idle;
        let state = state.dismiss_error();
        assert!(state.is_idle());
    }

    #[test]
    fn test_dismiss_error_from_hosting_is_noop() {
        let state = RoomState::start_hosting(
            "code".into(),
            "ticket".into(),
            test_topic_id(),
            test_namespace_id(),
        );
        let state = state.dismiss_error();
        assert!(state.is_hosting());
    }

    #[test]
    fn test_leave_error_returns_to_idle_via_dismiss() {
        let state = RoomState::fail("oops".into());
        let state = state.dismiss_error();
        assert!(state.is_idle());
    }

    #[test]
    fn test_room_code_none_for_idle() {
        let state = RoomState::Idle;
        assert_eq!(state.room_code(), None);
    }

    #[test]
    fn test_topic_id_none_for_error() {
        let state = RoomState::fail("error".into());
        assert_eq!(state.topic_id(), None);
    }

    #[test]
    fn test_joining_state_not_merged() {
        let state = RoomState::start_joining("partyxyz".into());
        assert_eq!(state.room_code(), None);
    }

    #[test]
    fn test_namespace_id_none_for_idle() {
        let state = RoomState::Idle;
        assert_eq!(state.namespace_id(), None);
    }

    #[test]
    fn test_namespace_id_none_for_error() {
        let state = RoomState::fail("error".into());
        assert_eq!(state.namespace_id(), None);
    }

    #[test]
    fn test_namespace_id_available_in_hosting() {
        let ns_id = test_namespace_id();
        let state = RoomState::start_hosting(
            "code".into(),
            "ticket".into(),
            test_topic_id(),
            ns_id,
        );
        assert_eq!(state.namespace_id(), Some(ns_id));
    }

    #[test]
    fn test_namespace_id_available_in_joined() {
        let ns_id = test_namespace_id();
        let state = RoomState::start_joining("ticket".into());
        let state = state.join("code".into(), test_topic_id(), ns_id);
        assert_eq!(state.namespace_id(), Some(ns_id));
    }
}
