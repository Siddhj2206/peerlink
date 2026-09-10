use serde::{Deserialize, Serialize};

/// A chat message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatMessage {
    pub author: String,
    pub content: String,
    pub timestamp: u64,
}

impl ChatMessage {
    pub fn new(author: String, content: String) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Self { author, content, timestamp: ts }
    }

    pub fn new_with_ts(author: String, content: String, timestamp: u64) -> Self {
        Self { author, content, timestamp }
    }

    /// Encode message to bytes via postcard.
    pub fn encode(&self) -> Vec<u8> {
        postcard::to_stdvec(self).expect("ChatMessage serialization must not fail")
    }

    /// Decode message from postcard bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, postcard::Error> {
        postcard::from_bytes(bytes)
    }

    /// Sortable doc key: `chat/<8-byte-be-timestamp>/<author-hex>`
    pub fn doc_key(&self) -> Vec<u8> {
        let author_hex = hex::encode(self.author.as_bytes());
        let mut key = Vec::with_capacity(5 + 8 + 1 + author_hex.len());
        key.extend_from_slice(b"chat/");
        key.extend_from_slice(&self.timestamp.to_be_bytes());
        key.push(b'/');
        key.extend_from_slice(author_hex.as_bytes());
        key
    }

    /// Parse a doc key back into timestamp and author hex.
    pub fn parse_key(key: &[u8]) -> Option<(u64, String)> {
        if !key.starts_with(b"chat/") || key.get(13) != Some(&b'/') {
            return None;
        }
        let ts_bytes: [u8; 8] = key[5..13].try_into().ok()?;
        let timestamp = u64::from_be_bytes(ts_bytes);
        let author_bytes = key[14..].to_vec();
        let author = String::from_utf8(author_bytes).ok()?;
        Some((timestamp, author))
    }

    /// Doc key prefix for querying all messages.
    pub fn key_prefix() -> &'static [u8] {
        b"chat/"
    }
}

/// Trait for the chat engine abstraction.
pub trait ChatEngine {
    /// Send a message with the given content (uses stored author).
    fn send(&mut self, content: String);
    /// Drain all received messages since last call.
    fn drain_messages(&mut self) -> Vec<ChatMessage>;
}

/// Local in-memory chat engine — messages stay local.
#[derive(Default)]
pub struct LocalChatEngine {
    author: String,
    incoming: Vec<ChatMessage>,
}

impl LocalChatEngine {
    pub fn new(author: String) -> Self {
        Self { author, incoming: Vec::new() }
    }
}

impl ChatEngine for LocalChatEngine {
    fn send(&mut self, content: String) {
        let msg = ChatMessage::new(self.author.clone(), content);
        self.incoming.push(msg);
    }

    fn drain_messages(&mut self) -> Vec<ChatMessage> {
        std::mem::take(&mut self.incoming)
    }
}

/// Iroh-docs backed chat engine — requires a running iroh docs engine.
#[allow(dead_code)]
pub struct IrohChatEngine {
    author: iroh_docs::AuthorId,
    author_short: String,
    doc: iroh_docs::api::Doc,
    buffer: Vec<ChatMessage>,
}

#[allow(dead_code)]
impl IrohChatEngine {
    pub fn new(author: iroh_docs::AuthorId, doc: iroh_docs::api::Doc) -> Self {
        let author_short = author.fmt_short();
        Self { author, author_short, doc, buffer: Vec::new() }
    }
}

#[allow(dead_code)]
impl ChatEngine for IrohChatEngine {
    fn send(&mut self, content: String) {
        let msg = ChatMessage::new(self.author_short.clone(), content);
        let key = msg.doc_key();
        let bytes = msg.encode();
        let doc = self.doc.clone();
        let author = self.author;
        tokio::spawn(async move {
            let _ = doc.set_bytes(author, key, bytes).await;
        });
        self.buffer.push(msg);
    }

    fn drain_messages(&mut self) -> Vec<ChatMessage> {
        std::mem::take(&mut self.buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ChatMessage serialization ──────────────────────────────────

    #[test]
    fn test_message_round_trip_bytes() {
        let msg = ChatMessage::new_with_ts("alice".into(), "hello".into(), 1000);
        let bytes = msg.encode();
        let decoded = ChatMessage::decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_message_content_preserved() {
        let msg = ChatMessage::new_with_ts("bob".into(), "hi there!".into(), 2000);
        let decoded = ChatMessage::decode(&msg.encode()).unwrap();
        assert_eq!(decoded.content, "hi there!");
    }

    #[test]
    fn test_message_author_preserved() {
        let msg = ChatMessage::new_with_ts("charlie".into(), "yo".into(), 3000);
        let decoded = ChatMessage::decode(&msg.encode()).unwrap();
        assert_eq!(decoded.author, "charlie");
    }

    #[test]
    fn test_message_timestamp_preserved() {
        let msg = ChatMessage::new_with_ts("dave".into(), "test".into(), 999999);
        let decoded = ChatMessage::decode(&msg.encode()).unwrap();
        assert_eq!(decoded.timestamp, 999999);
    }

    #[test]
    fn test_decode_empty_bytes_fails() {
        let result = ChatMessage::decode(b"");
        assert!(result.is_err());
    }

    // ── Sortable key format ────────────────────────────────────────

    #[test]
    fn test_doc_key_starts_with_chat_prefix() {
        let msg = ChatMessage::new("alice".into(), "hello".into());
        let key = msg.doc_key();
        assert!(key.starts_with(b"chat/"));
    }

    #[test]
    fn test_doc_key_contains_timestamp() {
        let msg = ChatMessage::new_with_ts("alice".into(), "hi".into(), 42);
        let key = msg.doc_key();
        assert_eq!(key[5..13], 42u64.to_be_bytes());
    }

    #[test]
    fn test_doc_key_ends_with_author_hex() {
        let msg = ChatMessage::new_with_ts("alice".into(), "hi".into(), 10);
        let key = msg.doc_key();
        let expected_hex = hex::encode(b"alice");
        assert!(key.ends_with(expected_hex.as_bytes()));
    }

    #[test]
    fn test_doc_keys_sort_by_timestamp() {
        let early = ChatMessage::new_with_ts("alice".into(), "first".into(), 100);
        let late = ChatMessage::new_with_ts("bob".into(), "second".into(), 200);
        assert!(early.doc_key() < late.doc_key());
    }

    #[test]
    fn test_doc_key_parse_round_trip() {
        let msg = ChatMessage::new_with_ts("alice".into(), "hi".into(), 12345);
        let key = msg.doc_key();
        let (ts, author) = ChatMessage::parse_key(&key).unwrap();
        assert_eq!(ts, 12345);
        assert_eq!(author, hex::encode(b"alice"));
    }

    #[test]
    fn test_parse_key_invalid_short() {
        assert!(ChatMessage::parse_key(b"chat").is_none());
        assert!(ChatMessage::parse_key(b"other/foo").is_none());
        assert!(ChatMessage::parse_key(b"").is_none());
    }

    #[test]
    fn test_key_prefix_is_correct() {
        assert_eq!(ChatMessage::key_prefix(), b"chat/");
    }

    // ── Local chat engine ──────────────────────────────────────────

    #[test]
    fn test_local_engine_send_and_drain() {
        let mut engine = LocalChatEngine::new("alice".into());
        assert!(engine.drain_messages().is_empty());
        engine.send("hello".into());
        let msgs = engine.drain_messages();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].author, "alice");
        assert_eq!(msgs[0].content, "hello");
    }

    #[test]
    fn test_local_engine_multiple_messages() {
        let mut engine = LocalChatEngine::new("bob".into());
        engine.send("first".into());
        engine.send("second".into());
        let msgs = engine.drain_messages();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].content, "first");
        assert_eq!(msgs[1].content, "second");
    }

    #[test]
    fn test_local_engine_drain_clears_buffer() {
        let mut engine = LocalChatEngine::new("carol".into());
        engine.send("msg".into());
        let _ = engine.drain_messages();
        assert!(engine.drain_messages().is_empty());
    }

    #[test]
    fn test_local_engine_multiple_drains() {
        let mut engine = LocalChatEngine::new("dave".into());
        engine.send("a".into());
        let _ = engine.drain_messages();
        engine.send("b".into());
        let msgs = engine.drain_messages();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "b");
    }
}
