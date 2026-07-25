use iroh_gossip::TopicId;
use iroh_tickets::{ParseError, Ticket};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyTicket {
    version: u8,
    topic_id: [u8; 32],
}

impl PartyTicket {
    pub fn new(topic_id: TopicId) -> Self {
        Self {
            version: 0,
            topic_id: *topic_id.as_bytes(),
        }
    }

    pub fn topic_id(&self) -> TopicId {
        TopicId::from_bytes(self.topic_id)
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

    #[test]
    fn test_party_ticket_round_trip_bytes() {
        let topic_id = test_topic_id();
        let ticket = PartyTicket::new(topic_id);
        let bytes = ticket.encode_bytes();
        let decoded = PartyTicket::decode_bytes(&bytes).unwrap();
        assert_eq!(ticket, decoded);
        assert_eq!(decoded.topic_id(), topic_id);
    }

    #[test]
    fn test_party_ticket_round_trip_string() {
        let topic_id = test_topic_id();
        let ticket = PartyTicket::new(topic_id);
        let s = ticket.encode_string();
        assert!(s.starts_with("party"), "string should start with 'party', got: {s}");
        let decoded = PartyTicket::decode_string(&s).unwrap();
        assert_eq!(ticket, decoded);
    }

    #[test]
    fn test_party_ticket_parse() {
        let topic_id = test_topic_id();
        let ticket = PartyTicket::new(topic_id);
        let s = ticket.to_string_encoded();
        let parsed = PartyTicket::parse(&s).unwrap();
        assert_eq!(parsed.topic_id(), topic_id);
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
        let ticket1 = PartyTicket::new(topic_id);
        let ticket2 = PartyTicket::new(topic_id);
        assert_eq!(ticket1.encode_string(), ticket2.encode_string());
    }
}
