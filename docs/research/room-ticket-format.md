# Room / Ticket / Invitation Format Research

## 1. Iroh's Native Ticket System (`iroh-tickets`)

### The `Ticket` trait (`iroh_tickets` v1.0.0)

The canonical iroh ticket abstraction lives in the `iroh-tickets` crate. It defines a trait:

```rust
pub trait Ticket: Sized {
    const KIND: &'static str;          // e.g. "endpoint", "blob", "doc", "room"
    fn encode_bytes(&self) -> Vec<u8>;
    fn decode_bytes(bytes: &[u8]) -> Result<Self, ParseError>;
    fn encode_string(&self) -> String;  // default: KIND + base32(encode_bytes)
    fn decode_string(s: &str) -> Result<Self, ParseError>;
}
```

**String format**: lowercase `KIND` prefix + RFC 4648 base32 (no padding) of postcard-encoded bytes.

Examples from the ecosystem:
| Kind | Example |
|------|---------|
| `endpoint` | `endpointacxfr74igmsbvsbnn73wcecg5vt3kbzncqwfrdiampuufwnhkublmaq...` |
| `blob` | `blob<base32(postcard(hash + addr + format))>` |
| `doc` | `doc<base32(postcard(doc_id + addr))>` |
| `room` | `room<base32(postcard(topic_id + bootstrap_peers))>` |

All ship the same pattern: **postcard** for binary encoding, **base32 (no pad, lowercase)** for string transport. This is the convention downstream crates should follow.

### Blob Ticket Fields (from `iroh-blobs` / `go-iroh`)

A `BlobTicket` bundles:
- `EndpointAddr` (who has the data)
- `Hash` (BLAKE3 hash of the blob)
- `BlobFormat` (raw vs hash-sequence / recursive)

The Go implementation confirms the same binary format: postcard payload, base32 string form.

### Endpoint Ticket Fields

- `EndpointAddr` = `EndpointId` (32-byte ed25519 public key) + `BTreeSet<TransportAddr>` (relay URLs + direct IP addresses)

Postcard wire format is versioned via a tagged enum `TicketWireFormat::Variant1(...)`.

---

## 2. Rooms in iroh-live

### `LiveTicket` (`iroh-live/src/ticket.rs`)

Used for **publish/subscribe** of a single broadcast.

**Fields**:
```rust
pub struct LiveTicket {
    pub endpoint: EndpointAddr,
    pub broadcast_name: String,
    pub relay_urls: Vec<String>,  // optional
}
```

**Notably**: This does **not** use the `iroh_tickets::Ticket` trait. Instead it uses a **custom URI scheme**:

```
iroh-live:<base64url(postcard(EndpointAddr))>/<broadcast_name>
```

Also accepts legacy format: `name@<base32(postcard(EndpointAddr))>`.

The `base64url` encoding choice here is interesting — it avoids the uppercase/lowercase ambiguity of base32 and is more compact. But it breaks from the iroh ecosystem convention.

### `RoomTicket` (`iroh-live/src/rooms.rs`, inner `mod ticket`)

Used for **multi-party rooms** with gossip-based coordination.

**Fields**:
```rust
pub struct RoomTicket {
    pub topic_id: TopicId,           // iroh_gossip::TopicId (32 bytes)
    pub bootstrap: Vec<EndpointId>,  // known peers for initial gossip
}
```

This **does** use `iroh_tickets::Ticket`:
- `KIND = "room"`
- `encode_bytes` → postcard serialization
- `encode_string` → default base32
- Round-trips via `FromStr` → `iroh_tickets::Ticket::decode_string`

### Room architecture (no built-in "room" concept)

iroh-live's "room" is **not** a built-in iroh primitive. It's an application-level construct built on:
1. **iROH Gossip** (`iroh_gossip`) — peers subscribe to a shared `TopicId`
2. **iROH Smol KV** (`iroh_smol_kv`) — a small key-value store over gossip for peer state announcements (broadcast names, display names)
3. **MoQ** (`iroh-moq`) — actual media streaming via QUIC

When a peer joins, it:
1. Subscribes to the gossip topic identified by `TicketId`
2. Announces its broadcast names via smol-kv
3. Gossip propagates peer state to all members
4. Each member subscribes to announced broadcasts over MoQ

The `RoomTicket` is purely the bootstrap mechanism — it tells new peers what gossip topic to join and who's already there.

---

## 3. Industry Pattern Comparison

| System | Format | Length | Human-readable? | Notes |
|--------|--------|--------|----------------|-------|
| **iROH (canonical)** | `kind` + base32(postcard) | ~150-300 chars | No (dense base32) | Ecosystem-compatible, QR-friendly |
| **iROH LiveTicket** | `iroh-live:` + base64url + `/` + name | ~150-250 chars | Partially (name visible) | Breaks from iroh convention |
| **Magic Wormhole** | Short code (e.g. `7-cactus-overcoat`) | ~20 chars | Yes | Requires rendezvous server |
| **BitTorrent magnet** | `magnet:?xt=urn:btih:<infohash>&dn=<name>` | ~60-100 chars | Partially | URI scheme |
| **Signal/WhatsApp call** | `https://signal.gg/...` or `tsignal://...` | ~50-100 chars | No (opaque) | Uses signaling server |
| **WebRTC SDP** | JSON or string with session description | ~500-2000 chars | No | Verbose, needs signaling channel |
| **IPFS** | `/ipfs/<cid>` or `ipfs://<cid>` | ~60-100 chars | No | Multiaddr-based |

### Key insight for watch party

- We need **human shareability** (copy-paste, QR code, possibly NFC)
- We need the ticket to carry: **endpoint address** + **room/topic identifier** + **optional metadata** (media ID, timestamp offset)
- We'll have **short-lived sessions** where both peers are online simultaneously → tickets are ideal
- We should prefer **ecosystem compatibility** with iroh

---

## 4. How iroh's Blob Ticket Works

The canonical iroh ticket pattern (from `docs.iroh.computer`):

1. **Creator side**: `BlobTicket::new(endpoint_addr, hash, format)` → `ticket.to_string()` → `blob<base64ish>`
2. **Receiver side**: `let ticket: BlobTicket = input.parse()?` → `downloader.download(ticket.hash(), ...)`
3. **String form**: `blob` prefix + base32-lowercase-no-pad of postcard-encoded bytes

The docs emphasize:
> **Use tickets when** bootstrapping without central coordination, manual sharing (QR, copy-paste), short-lived sessions.
> **Don't use tickets when** you have a database/server, long-lived connections, or can cache `EndpointId`s.

For rooms specifically: the iroh docs recommend using `EndpointId`s directly when you have gossip, since gossip itself is a coordination mechanism. But we still need a bootstrap token for the first connection.

---

## 5. Serialization Format Comparison

| Encoding | Pros | Cons | Used by |
|----------|------|------|---------|
| **base32** (RFC 4648, no pad) | Case-insensitive, no ambiguous chars, standard in iroh | ~60% larger than raw, longer strings | iroh canonical tickets |
| **base64url** (no pad) | Compact (~33% overhead), URL-safe | Case-sensitive, `-` and `_` chars | iroh-live LiveTicket |
| **base58** (Bitcoin-style) | No ambiguous chars, compact | Non-standard, slower | Bitcoin addresses |
| **Bech32** | Error detection, human-readable prefix | Bitcoin-specific, more complex | Bitcoin (SegWit) |
| **hex** | Simple, readable | 2× size, no density | Debug/display only |

### Recommended: Stick with iroh's base32 + postcard convention

- Our ticket will implement the `iroh_tickets::Ticket` trait
- Use `KIND = "watchparty"` (or similar) prefix
- Use postcard for binary serialization (already a dependency in iroh ecosystem)
- Use base32-no-pad-lowercase for string form
- This is the convention `RoomTicket` in iroh-live already follows

---

## 6. Recommendation

### Format: Custom ticket via `iroh_tickets::Ticket` trait

```
KIND = "party"  (or "watch")

Fields (postcard-serialized):
  version:    u8              = 0
  endpoint:   EndpointAddr    (or just EndpointId for shorter tickets)
  topic_id:   TopicId         (32-byte gossip topic)
  media_id:   String          (optional — the broadcast/stream name)
  passphrase: Hash256         (optional — room password hash)

String form:
  party<base32(postcard(payload))>

Example:
  partyab3e7f...gq5a
```

### Rationale

1. **Ecosystem alignment**: Using the `Ticket` trait means our tickets are compatible with iroh tooling (ticket.iroh.computer debugger, CLI parsers). The `RoomTicket` precedent in iroh-live proves this works.

2. **Two-tier ticket system**:
   - **Short ticket** (uses `EndpointId` only, relies on relay for address resolution): ~100-120 chars
   - **Long ticket** (uses full `EndpointAddr` with embedded direct addresses): ~200-300 chars

3. **Postcard over alternatives**: Already a workspace dependency, gives ~1.5× smaller than JSON, zero-copy deserialization available, supports `#[serde(deny_unknown_fields)]` for forward compatibility.

4. **Room passphrase support**: Optional 32-byte hash in the ticket allows password-protected rooms without needing a coordinator. The passphrase would be a BLAKE3 hash of a human-readable word(s) — the ticket embeds the hash, users share the word out-of-band and the app verifies the hash matches.

5. **QR-code friendly**: Even the long form (~300 chars) fits comfortably in a QR code (QR v4 can hold ~4296 alphanumeric chars at L-level ECC).

### Key differences from iroh-live's approach

| Aspect | iroh-live `RoomTicket` | Proposed for peerlink |
|--------|----------------------|----------------------|
| Ticket trait | ✅ Yes | ✅ Yes |
| KIND | `"room"` | `"party"` |
| Fields | `topic_id` + `bootstrap` | `topic_id` + `endpoint` + optional `media_id` + optional `passphrase` |
| Bootstrap list | Included | Not included (gossip bootstraps from endpoint addr directly) |
| Version field | No | Yes (`u8` prefix for forward compat) |
| Passphrase support | No | Optional (BLAKE3 hash) |

The iroh-live `RoomTicket` embeds `bootstrap: Vec<EndpointId>` so new peers know who to contact for initial gossip. Our approach can skip this if we use iroh's relay + address lookup, or embed it optionally. Given that iroh's docs say "use `EndpointId`s directly when you have gossip", we can keep tickets shorter by omitting bootstrap peers and relying on iroh's discovery mechanisms.
