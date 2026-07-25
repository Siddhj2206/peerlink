use iroh_docs::NamespaceId;
use iroh_gossip::TopicId;
use iroh_tickets::{ParseError, Ticket};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyTicket {
    version: u8,
    topic_id: [u8; 32],
    namespace_id: [u8; 32],
}

impl PartyTicket {
    pub fn new(topic_id: TopicId, namespace_id: NamespaceId) -> Self {
        Self {
            version: 0,
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

    pub fn parse(s: &str) -> Result<Self, ParseError> {
        Self::decode_string(s)
    }

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
    fn test_party_ticket_round_trip_bytes() {
        let topic_id = test_topic_id();
        let ns_id = test_namespace_id();
        let ticket = PartyTicket::new(topic_id, ns_id);
        let bytes = ticket.encode_bytes();
        let decoded = PartyTicket::decode_bytes(&bytes).unwrap();
        assert_eq!(ticket, decoded);
        assert_eq!(decoded.topic_id(), topic_id);
        assert_eq!(decoded.namespace_id(), ns_id);
    }

    #[test]
    fn test_party_ticket_round_trip_string() {
        let topic_id = test_topic_id();
        let ns_id = test_namespace_id();
        let ticket = PartyTicket::new(topic_id, ns_id);
        let s = ticket.encode_string();
        assert!(s.starts_with("party"), "string should start with 'party', got: {s}");
        let decoded = PartyTicket::decode_string(&s).unwrap();
        assert_eq!(ticket, decoded);
    }

    #[test]
    fn test_party_ticket_parse() {
        let topic_id = test_topic_id();
        let ns_id = test_namespace_id();
        let ticket = PartyTicket::new(topic_id, ns_id);
        let s = ticket.to_string_encoded();
        let parsed = PartyTicket::parse(&s).unwrap();
        assert_eq!(parsed.topic_id(), topic_id);
        assert_eq!(parsed.namespace_id(), ns_id);
    }

    #[test]
    fn test_invalid_prefix_rejected() {
        let result = PartyTicket::decode_string("wrongprefixabc123");
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_base32_rejected() {
        let result = PartyTicket::decode_string("party!!!invalid!!!");
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_string_rejected() {
        let result = PartyTicket::decode_string("");
        assert!(result.is_err());
    }

    #[test]
    fn test_deterministic_encoding() {
        let topic_id = test_topic_id();
        let ns_id = test_namespace_id();
        let ticket1 = PartyTicket::new(topic_id, ns_id);
        let ticket2 = PartyTicket::new(topic_id, ns_id);
        assert_eq!(ticket1.encode_string(), ticket2.encode_string());
    }
}
