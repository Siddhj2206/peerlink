# PeerLink Tauri architecture

**Status:** Agreed design (pending confirmation)
**Audience:** Engineering
**Map:** [#12 Wayfinder: local-only Tauri Party rewrite](https://github.com/Siddhj2206/peerlink/issues/12)
**Decides:** [#14 Choose the first Party participant topology](https://github.com/Siddhj2206/peerlink/issues/14) → staged desktop-first

---

## 1. Purpose

Replace the single-binary `eframe`/egui app with a **Tauri v2** desktop shell, a **React + TypeScript (Vite)** webview frontend, and the reusable logic in a library crate, `peerlink-core`.

The webview is the player. Rust moves bytes and owns the truth; it never decodes or renders video.

## 2. Non-negotiable

**P2P / decentralization.** Every Guest connects directly to the Host over iroh (hole-punched QUIC). Media moves via iroh-blobs; control and chat via iroh-gossip. A relay appears only as a non-persistent fallback, and only in the *staged* browser path. No media server, no persistence, no central coordination in the default topology.

## 3. Constraints (from #12)

- One Host streams a local H.264/AAC MP4 to a Party of at most five Guests.
- Direct encrypted P2P preferred; non-persistent encrypted Relay is the only fallback.
- No persistent uploads, media library, transcoding, or large-room SFU.
- The existing eframe playback is **not** a migration target. Assume the current media/network code does not work and is not tested.

## 4. Decisions (grill outcomes)

| # | Decision |
|---|---|
| Layout | Workspace: `crates/peerlink-core`, `src-tauri/`, `web/`. One core crate. |
| Core shape | One role-agnostic `Party` aggregate + `PartyRole`; small public surface. |
| Truth | Core owns authoritative room + playback state; webview renders `PartyEvent`s. |
| Concurrency | `Arc<Mutex<Party>>` + `tokio::sync::broadcast<PartyEvent>`. |
| Playback | The **webview** decodes and renders; Rust does not. |
| Salvage | Fresh rewrite. Reuse only self-evidently pure code after re-validation; don't carry the old test suite as a safety net. |
| Topology (#14) | **Staged desktop-first**: first supported topology is all-desktop Tauri; keep the browser path as a later adapter. |
| Media delivery | Progressive fetch — Rust serves byte ranges; `<video>` streams and seeks while fetching. |
| Byte transport | `iroh-blobs` verified **range** requests. |
| Control/chat | `iroh-gossip`, ephemeral, no persistence. |
| Media source | A **loopback HTTP server** (Axum) with correct Range/206; one server for Host and Guest. |
| Ticket | Extended to carry `NodeAddr + blob Hash + TopicId`. |
| Sync | Host-authoritative state broadcast + threshold correction (~250 ms). |
| eframe | Parked as `crates/peerlink-egui` until parity, then deleted. |

## 5. Target shape

```
peerlink/
├─ Cargo.toml                     # [workspace]
├─ crates/
│  ├─ peerlink-core/              # library: ALL behaviour, zero UI/Tauri deps
│  │  └─ src/
│  │     ├─ lib.rs
│  │     ├─ domain/               # room, ticket, room_code (pure)
│  │     ├─ chat.rs               # ChatMessage + gossip-backed engine
│  │     ├─ media.rs              # MP4 metadata / faststart check (new, minimal)
│  │     ├─ transport/            # endpoint, blobs (provider + range getter), gossip
│  │     └─ session.rs            # Party aggregate + PartyEvent
│  └─ peerlink-egui/              # TEMPORARY: parked eframe app; deleted at parity
├─ src-tauri/
│  ├─ Cargo.toml                  # depends on peerlink-core
│  ├─ build.rs
│  ├─ tauri.conf.json
│  ├─ capabilities/default.json
│  └─ src/{main.rs, lib.rs, commands.rs, events.rs, model.rs, state.rs}
└─ web/
   ├─ package.json  vite.config.ts  index.html
   └─ src/{main.tsx, App.tsx, party-client/, useParty.ts, views/{Home,Party}.tsx}
```

**Why one core crate.** Splitting `domain`/`media`/`transport` adds seams nothing varies across yet. Split only when a second real consumer appears.

**Why `web/`.** Keeps "Rust `src/`" and "TS `src/`" from colliding; `tauri.conf.json` points `frontendDist` at `web/dist`.

## 6. Salvage policy

Because nothing in the current tree is trusted:

- **Reuse only after re-validation:** `domain` (room code generation, ticket serialization, `RoomState`) — small, pure, and cheap to re-derive if it misbehaves.
- **Rewrite from scratch:** media metadata, blob transport, gossip, session orchestration. These are exactly the parts being redesigned.
- **Old tests are not a safety net.** New tests are written against the new interfaces as those interfaces are built, so a green suite means something.
- **Drop** `decode.rs`, `live.rs`, `rusty-codecs`, `wgpu`, and all `iroh-live` dependencies. (This also removes a landmine: `iroh-live` merged a v2 rewrite on 2026-09-09 that deletes `rusty-codecs`; the repo's lock pins the old rev, but any `cargo update` would break it.)

## 7. Media path

```
webview   <video src="http://127.0.0.1:<port>/media/<token>">
               │  HTTP Range → 206
src-tauri     loopback Axum server  ──►  MediaSource
              (127.0.0.1, ephemeral port,    ├─ LocalFile(path)      (Host)
               per-launch token)             └─ IrohBlob{...}         (Guest, remote range fetch)
```

**Why not the Tauri asset/custom protocol.** On Linux, WebKitGTK **cannot play media from a custom URI scheme or `asset://`** — it returns `MEDIA_ERR_SRC_NOT_SUPPORTED` even though `fetch()` returns 200. This is [WebKit #146351](https://bugs.webkit.org/show_bug.cgi?id=146351), open since 2015, tracked as [Tauri #3725](https://github.com/tauri-apps/tauri/issues/3725) (`priority: 1 high`). A loopback HTTP server is the only approach verified to work across Windows, macOS, and Linux. This is almost certainly why the previous attempt's video never played.

**How it works.**

- The Host registers the MP4 as an iroh blob (in-memory/temp store — non-persistent) and puts its hash + endpoint address in the ticket.
- Each Guest's `MediaSource::IrohBlob` fulfills an HTTP `Range` by issuing an iroh-blobs verified range request to the Host. iroh-blobs rounds byte ranges up to 16 KiB BLAKE3 chunk groups and validates the data in transit. ([iroh blobs](https://docs.iroh.computer/protocols/blobs), [protocol docs](https://docs.rs/iroh-blobs/latest/iroh_blobs/protocol/index.html))
- The Host's own window uses `MediaSource::LocalFile` through the same server, so there is exactly one player path.
- The server must implement `HEAD`, `Range` → `206` + `Content-Range` + `Content-Length`, `Accept-Ranges: bytes`, and `416` for unsatisfiable ranges. Response chunks are bounded (Tauri's own asset protocol clamps to 1 MB). _Do not copy `nitiksh/tauri-video-streaming-template` — it advertises `Accept-Ranges` but ignores `Range`._
- Bind `127.0.0.1` only, gate every request behind a per-launch random token, and allow the origin in CSP (`media-src`).
- `moov`-at-end MP4s cannot be progressively played. A small metadata check at share time detects this and surfaces a clear error; normalization/transcoding is out of scope.

**Escape hatches, not first choices:** MSE/fMP4 (if a non-progressive source appears), WebCodecs (WebKitGTK ≥ 2.44, bypasses the media loader), native surface (GStreamer/mpv — no macOS support in the only mature plugin). None are needed for the first slice.

**Known platform pitfalls** (see §16 references): Linux AppImage needs `bundle.linux.appimage.bundleMediaFramework = true`; `blob:` is not a universal fix; WebTransport (needed only for the staged browser path) is new enough that older WebKitGTK lacks it.

## 8. Control, sync, and chat

All on **iroh-gossip**, scoped to the topic id in the ticket. No persistence; a late joiner receives current state on join.

```rust
enum ControlMessage {
    PlaybackState { playing: bool, position_ms: u64, epoch_ms: u64, generation: u64 },
    Chat(ChatMessage),
    RequestState,
    PeerHello { display_name: String },
}
```

The Host is authoritative. It broadcasts `PlaybackState` on every play/pause/seek and periodically (~1–2 s). A Guest computes drift against the received `{position, epoch}` and **seeks only when drift exceeds ~250 ms** — imperceptible for small drift, self-healing for large, graceful when a Guest buffers. (Continuous rate-nudging via `playbackRate` is a possible later refinement; it is not needed now.)

Chat is a `ControlMessage` variant with no history. Reintroduce persistence only if cross-session history becomes a requirement.

## 9. Ticket and identity

`PartyTicket` is extended to carry everything needed to connect and fetch, so one copy-pasteable string suffices:

```rust
struct PartyTicket {
    version: u8,
    host: EndpointAddr,   // iroh endpoint address (hole-punch info + relay fallback)
    blob: Hash,           // BLAKE3 hash of the MP4
    topic: TopicId,       // gossip topic for control/chat
}
```

Serialized with `postcard` via `iroh-tickets` (base32), consistent with the current format. Display names for chat are per-session, carried in `PeerHello`; there is no account model.

## 10. Core interface

```rust
// crates/peerlink-core/src/lib.rs
pub mod domain;
pub mod chat;
pub mod media;
pub mod transport;
mod session;

pub use chat::ChatMessage;
pub use domain::{PartyTicket, RoomState};
pub use media::MediaInfo;
pub use session::{Party, PartyEvent, PartyInfo, PartyRole};
```

```rust
// session.rs (sketch)
pub struct Party { /* room, ticket, endpoint, blob provider/getter, gossip, playback clock */ }

impl Party {
    pub async fn host(display_name: String, media: PathBuf) -> Result<(Self, PartyTicket)>;
    pub async fn join(ticket: &str, display_name: String) -> Result<Self>;
    pub fn info(&self) -> PartyInfo;
    pub async fn leave(&mut self) -> Result<()>;

    pub fn send_chat(&mut self, text: String) -> Result<()>;

    // Host-authoritative playback.
    pub fn play(&mut self);
    pub fn pause(&mut self);
    pub fn seek(&mut self, position: Duration);
    pub fn position(&self) -> Duration;

    /// URL the webview should point <video> at (loopback server token).
    pub fn media_url(&self) -> Option<String>;

    pub fn events(&self) -> broadcast::Receiver<PartyEvent>;
}

pub enum PartyEvent {
    Room(RoomState),
    PeerJoined { id: String, display_name: String },
    PeerLeft { id: String },
    Chat(ChatMessage),
    Playback { playing: bool, position: Duration },
    Error(String),
}
```

`Party` is the deep module: room lifecycle, endpoint, blob provider/getter, gossip routing, and the playback clock behind one interface. It is shared as `Arc<Mutex<Party>>`; async Tauri commands run off the UI thread, so a mutex held for microseconds only contends with other commands.

## 11. Rust ↔ JS contract

The Tauri layer is a translation adapter: it owns the event bridge and the managed state that keeps the loopback server (owned by `peerlink-core`) alive across commands — no domain state of its own.

Commands: `create_room({mediaPath})`, `join_room({ticket, displayName})`, `leave_room()`, `play()`, `pause()`, `seek({positionMs})`, `send_chat({text})`, `get_state()`, `pick_media_file()` (via `tauri-plugin-dialog`).

Events: `room-changed`, `peer-joined`, `peer-left`, `chat-message`, `playback`, `error`.

The frontend must **not** hard-depend on Tauri IPC. All calls go through a `PartyClient` interface:

```ts
interface PartyClient {
  createRoom(mediaPath: string): Promise<PartyState>;
  joinRoom(ticket: string, displayName: string): Promise<PartyState>;
  play(): Promise<void>;
  pause(): Promise<void>;
  seek(positionMs: number): Promise<void>;
  sendChat(text: string): Promise<void>;
  subscribe(handler: (e: PartyEvent) => void): () => void;
}
```

A `TauriPartyClient` (`invoke` + `listen`) ships now. A browser `WebSocket`/WebTransport client can be added later without touching the views. This is the seam that preserves the staged browser option.

## 12. Frontend

React + TS + Vite in `web/`. Plain hooks: a `useParty()` hook holds `PartyState` and subscribes to events. Two views: `Home` (create/join) and `Party` (`<video>` + controls + chat). No router, no state library, no React Query until fan-out actually hurts. `Party` in the Rust core remains the source of truth.

## 13. Staged browser path

First supported topology is all-desktop. To add browser Guests later: the Host's loopback server is replaced/augmented by a WebTransport bridge (WebCodecs in the browser), and the frontend's `PartyClient` gets a browser implementation. The research notes this requires a relay with self-signed-cert pinning over plain HTTP, has no auth, and is browser-support-limited (Chromium; Firefox 153+; Safari per relay README is out). This is deliberately deferred.

## 14. Build sequence

1. **Spike (de-risk Linux media):** a minimal Tauri v2 app serving one local MP4 over loopback Axum with correct Range/206 into `<video>`. Prove playback **and seeking** on Linux. Nothing else.
2. **Workspace + core skeleton:** create the workspace, `peerlink-core` with `domain`/`media`/`transport`/`session`; re-derive pure domain code and test it.
3. **Transport:** endpoint + ticket (extended) + iroh-blobs provider/getter + gossip, tested between two in-process endpoints.
4. **Vertical slice:** two desktop Tauri processes — Host opens an MP4; Guest joins, fetches ranges over iroh-blobs, plays in `<video>`, with play/pause/seek synced. No chat.
5. **Chat** as a gossip message.
6. **Parity + cleanup:** reach feature parity, delete `peerlink-egui` and the eframe/egui dependencies.

## 15. Testing

- **Core:** unit tests for domain; two-endpoint integration tests for blob range fetch and gossip; `Party` tests through its interface. No display server, no webview, no network beyond loopback.
- **Tauri:** `tauri::test` invoking commands; an HTTP test hitting the media server's `Range`/`HEAD`/`416` paths against a temp file.
- **Frontend:** component tests with a fake `PartyClient`.

## 16. Reference implementations

- Loopback HTTP workaround in production: [block/buzz #2562](https://github.com/block/buzz/issues/2562); Tauri [#3725](https://github.com/tauri-apps/tauri/issues/3725).
- Range/206 handlers to imitate: Tauri [`examples/streaming`](https://github.com/tauri-apps/tauri/tree/dev/examples/streaming).
- Native-surface alternative (not chosen): [get-air/tauri-video-plugin](https://github.com/get-air/tauri-video-plugin), [FabianLars/tauri-v2-wgpu](https://github.com/FabianLars/tauri-v2-wgpu) (archived), [clearlysid/tauri-wgpu-cam](https://github.com/clearlysid/tauri-wgpu-cam).
- Live alternative (not chosen): [moq-dev/moq](https://github.com/moq-dev/moq), [iroh-live-relay](https://github.com/n0-computer/iroh-live).

## 17. Rejected alternatives

- **Native/GStreamer/mpv surface.** Rejected: the user wants the webview to own playback; the only mature plugin has no macOS support.
- **MoQ live streaming.** Rejected for a seekable local MP4: heavier, requires the iroh-live v2 migration, and offers no benefit for VOD.
- **`asset://` / custom URI scheme.** Rejected: broken for media on Linux.
- **`iroh-docs` for control.** Rejected: control is ephemeral; docs' host-led reconciliation is unused complexity.
- **Keeping Rust decode as a fallback.** Rejected: directly contradicts "let the browser do it" and reintroduces the untrusted pipeline.
- **A JS-side state store as source of truth.** Rejected: duplicates `Party`'s invariants and breaks host authority.
