//! The shareable Party ticket.
//!
//! A ticket is the one string a Host hands to a Guest. It is encoded with
//! `postcard` inside `iroh-tickets`' base32 envelope, so it carries a `party`
//! prefix and is safe to paste through any chat channel.

use iroh_docs::NamespaceId;
use iroh_gossip::TopicId;
use iroh_tickets::{ParseError, Ticket};
use serde::{Deserialize, Serialize};

/// What a Guest needs in order to reach a Host's room.
///
/// The ids are stored as raw bytes rather than the iroh newtypes so the wire
/// format stays explicit and stable across dependency upgrades.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyTicket {
    version: u8,
    topic_id: [u8; 32],
    namespace_id: [u8; 32],
}

impl PartyTicket {
    /// Current ticket format version.
    const VERSION: u8 = 0;

    pub fn new(topic_id: TopicId, namespace_id: NamespaceId) -> Self {
        Self {
            version: Self::VERSION,
            topic_id: *topic_id.as_bytes(),
            namespace_id: namespace_id.to_bytes(),
        }
    }

    pub fn topic_id(&self) -> TopicId {
        TopicId::from_bytes(self.topic_id)
    }

    pub fn namespace_id(&self) -> NamespaceId {
        NamespaceId::from(&self.namespace_id)
    }

    /// Parse a ticket string pasted by a Guest.
    pub fn parse(s: &str) -> Result<Self, ParseError> {
        Self::decode_string(s)
    }

    /// Encode to the pasteable string form.
    pub fn to_string_encoded(&self) -> String {
        self.encode_string()
    }
}

impl Ticket for PartyTicket {
    const KIND: &'static str = "party";

    fn encode_bytes(&self) -> Vec<u8> {
        postcard::to_stdvec(self).expect("PartyTicket serialization must not fail")
    }

    fn decode_bytes(bytes: &[u8]) -> Result<Self, ParseError> {
        postcard::from_bytes(bytes).map_err(Into::into)
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

    fn ticket() -> PartyTicket {
        PartyTicket::new(topic(), namespace())
    }

    #[test]
    fn bytes_round_trip() {
        let decoded = PartyTicket::decode_bytes(&ticket().encode_bytes()).unwrap();
        assert_eq!(decoded, ticket());
        assert_eq!(decoded.topic_id(), topic());
        assert_eq!(decoded.namespace_id(), namespace());
    }

    #[test]
    fn string_round_trip_keeps_the_party_prefix() {
        let s = ticket().encode_string();
        assert!(s.starts_with("party"), "missing prefix: {s}");
        assert_eq!(PartyTicket::decode_string(&s).unwrap(), ticket());
    }

    #[test]
    fn parse_accepts_its_own_encoding() {
        let parsed = PartyTicket::parse(&ticket().to_string_encoded()).unwrap();
        assert_eq!(parsed.topic_id(), topic());
        assert_eq!(parsed.namespace_id(), namespace());
    }

    #[test]
    fn encoding_is_deterministic() {
        assert_eq!(ticket().encode_string(), ticket().encode_string());
    }

    #[test]
    fn distinct_ids_produce_distinct_tickets() {
        let other = PartyTicket::new(TopicId::from_bytes([0x33; 32]), namespace());
        assert_ne!(ticket(), other);
        assert_ne!(ticket().encode_string(), other.encode_string());
    }

    #[test]
    fn malformed_strings_are_rejected() {
        assert!(PartyTicket::parse("").is_err());
        assert!(PartyTicket::parse("wrongprefixabc123").is_err());
        assert!(PartyTicket::parse("party!!!invalid!!!").is_err());
    }
}
