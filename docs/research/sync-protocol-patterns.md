# Sync/Control Protocol Patterns for P2P Watch Party Applications

Research conducted 2026-07-25. Sources: Jellyfin SyncPlay, Syncplay (classic),
Nobar Party (Open Watch Party), mpmp (Rust), PeerWatch (Go), and general
WebRTC watch-party implementations.

---

## 1. Jellyfin SyncPlay

**Architecture:** Client-server over WebSocket. Server maintains authoritative
group state; clients execute playback locally and report state.

### State Machine

Four states on the server (`GroupStateType`):

| State     | Meaning |
|-----------|---------|
| `Idle`    | No media loaded, clients should be stopped |
| `Waiting` | Waiting for all clients to be ready (loaded/buffered) before transitioning to Playing or Paused |
| `Playing` | Media loaded and unpaused |
| `Paused`  | Media loaded but paused |

### Message Format (API Endpoints)

| Endpoint | Payload |
|----------|---------|
| `POST /SyncPlay/Play` | _(empty)_ |
| `POST /SyncPlay/Pause` | _(empty)_ |
| `POST /SyncPlay/Unpause` | _(empty)_ |
| `POST /SyncPlay/Stop` | _(empty)_ |
| `POST /SyncPlay/Seek` | `{ PositionTicks: i64 }` |
| `POST /SyncPlay/Buffering` | `{ When: DateTime, PositionTicks: i64, IsPlaying: bool, PlaylistItemId: Guid }` |
| `POST /SyncPlay/Ping` | `{ Ping: i64 }` |
| `POST /SyncPlay/Ready` | `{ When: DateTime, PositionTicks: i64, IsPlaying: bool, PlaylistItemId: Guid }` |

The server broadcasts `SendCommand` to all group members with:
`{ GroupId, PlaylistItemId, When (DateTime), CommandType, PositionTicks, DateOfCommand }`

CommandType values: `Play`, `Pause`, `Unpause`, `Stop`, `Seek`.

### Clock Sync

NTP-lite: clients call `GET /SyncPlay/GetUtcTime` which returns the server's
UTC time along with client-reported timestamps for RTT calculation. The client
measures:
- `rtt = t4 - t1 - (t3 - t2)`
- `offset = t2 - t1 + rtt/2`

Keeps last ~20 measurements, picks the lowest-RTT sample. Starts in "greedy"
mode (rapid pings for baseline), then drops to one ping per ~30s.

Configurable tolerance: `TimeSyncOffset = 2000ms` (max clock offset error),
`MaxPlaybackOffset = 500ms` (max position error).

### Drift Correction

- Within ±500ms: no action (tolerance window)
- Beyond ±500ms: speed adjustment (playback rate) to catch up or slow down
- Large drift: hard seek
- On seek, the server pauses internally during the transition to prevent
  position races

### Joining Mid-Stream

1. Client joins group via WebSocket
2. Server sends `PlayQueueUpdate` with current position, `isPlaying` flag
3. If playing, client gets: `position + elapsed_time_since_last_activity`
4. Client seeks to that position and unpauses (or stays paused)

---

## 2. Syncplay (Classic) Protocol v1.2.7

**Architecture:** JSON over TCP. No explicit host — democratic control.

### Message Types

**Hello** (identify to server):
```json
{"Hello": {"username": "Bob", "password": "*md5*", "room": {"name": "SyncRoom"}, "version": "1.2.7"}}
```

**Set** (state changes):
```json
{"Set": {"user": {"Bob": {"room": {"name": "SyncRoom"}, "event": {"joined": true}}}}}
{"Set": {"file": {"duration": 596.458, "name": "BigBuckBunny.avi", "size": 220514438}}}
```

**State** (playback sync — sent continuously):
```json
{"State": {
  "playstate": {"paused": false, "position": 60.083, "setBy": "Bob", "doSeek": null},
  "ping": {"yourLatency": 0.012, "senderLatency": 0.012, "latencyCalculation": 1394588073.328},
  "ignoringOnTheFly": {"client": 1, "server": 1}
}}
```

**List** (request room state):
```json
{"List": null}
```

### Clock Sync

- `latencyCalculation`: client epoch timestamp (seconds as f64) included in
  every state message
- `yourLatency` / `senderLatency`: server reports RTT per-client
- `ignoringOnTheFly`: mutual acknowledgment pattern to prevent echo loops.
  When a client sends a change with `ignoringOnTheFly: {client: 1}`, it ignores
  incoming state messages until the server echoes `ignoringOnTheFly: {server: 1}` back,
  then the client acknowledges with `ignoringOnTheFly: {server: 1}`.

### Key Design Details

- No explicit host/leader — anyone can issue play/pause/seek
- Ping heartbeat every ~1s (timeout at 4s)
- File identity by (name hash, size hash, duration)
- Position in seconds as f64

---

## 3. Nobar Party (Open Watch Party)

**Architecture:** Browser extension (Chrome MV3) + Node.js WebSocket signaling
server. Democratic control.

### Message Format (JSON over WebSocket)

**Client → Server:**

| Type    | Fields |
|---------|--------|
| `join`  | `roomId?`, `nickname`, `create?` |
| `leave` | — |
| `play`  | `t` (video seconds), `at` (wall-clock ms) |
| `pause` | `t`, `at` |
| `seek`  | `t`, `at` |
| `url`   | `url` |
| `chat`  | `text` |
| `ping`  | `at` |

**Server → Client:**

| Type           | Fields |
|----------------|--------|
| `room`         | `roomId`, `selfId`, `members`, `url`, `playing`, `t`, `at` |
| `peer-joined`  | `id`, `nickname` |
| `peer-left`    | `id` |
| `play`/`pause`/`seek` | echoed with `fromId` |
| `pong`         | `at`, `serverAt` |
| `error`        | `code`, `message` |

### Clock Sync

- On join: send 3 `ping` frames 200ms apart
- Compute: `rtt = nowReceived - ping.at`, `offset = serverAt - (ping.at + rtt/2)`
- Keep the lowest-RTT sample
- Re-measure every 60s via `chrome.alarms`

### Drift / Echo Suppression

- 500ms echo-loop suppression: remote events don't rebroadcast via native DOM
  events. The extension intercepts and suppresses synthetic events.
- No continuous drift correction — play/pause/seek events carry wall-clock
  time and video position; clients use these to seek on receipt.

### Joining Mid-Stream

Server sends full room snapshot (`room` message) with current `playing` state,
`t` (position), and `at` (wall-clock). Client seeks to the position.

---

## 4. mpmp (Rust Playback Synchronization Framework)

**Architecture:** Client-server over TLS TCP. Server holds master timeline.
No periodic heartbeats — event-driven.

### Protocol Messages (Serde binary/JSON)

**Client → Server:**
```rust
enum ClientMessage {
    HelloV1 { current_properties: PropertiesV1 },
    ChangePropertiesV1 { properties: PropertiesV1 },
    GetPropertiesV1,
}
```

**Server → Client:**
```rust
enum ServerMessage {
    HelloV1,
    ChangePropertiesV1 { property: PropertiesV1 },
}
```

**Shared state blob:**
```rust
struct PropertiesV1 {
    paused: bool,
    time_pos: f64,    // seconds
    speed: f64,       // playback rate (1.0 = normal)
}
```

### Sync Strategy

- Clients push their `PropertiesV1` on every play/pause/seek event
- Server stores the latest and echoes to all other clients
- First client to connect initializes the lobby ("flying start")
- Late joiners receive current state via `ChangePropertiesV1` on `Hello`
- On seek, server automatically sets `paused = true` to prevent position races
  (since `time_pos` events keep coming during playback)
- No heartbeats, no continuous position polling

---

## 5. PeerWatch (Go, P2P over TCP)

**Architecture:** Full-mesh TCP, no central server. Host is source of truth
for sync. Used with mpv via JSON-RPC IPC.

### Protocol Messages (Binary, 10 types)

| Type ID | Name         | Key Fields |
|---------|--------------|------------|
| `0x01`  | HANDSHAKE    | `PeerID [16]byte`, `Version uint8` |
| `0x02`  | MANIFEST     | File metadata |
| `0x03`  | BITFIELD     | Chunk availability |
| `0x05`  | REQUEST      | `ChunkIndices []uint32` |
| `0x06`  | PIECE        | `ChunkIndex uint32`, `Data []byte` |
| `0x08`  | SYNC         | `PlaybackTime f64`, `State uint8`, `UnixMs int64` |
| `0x09`  | PEER_LIST    | `Addrs []string` |
| `0x0A`  | KEEPALIVE    | _(empty)_ |

State values: `StatePlaying = 0`, `StatePaused = 1`.

### Clock Sync

- SYNC message sent by host every 2 seconds
- Contains host's `PlaybackTime`, `State`, and `UnixMs` (wall clock)
- Client estimates network latency from `now - msg.UnixMs`
- Caps latency estimate at 0-5s (otherwise assumes clock desync, uses 0)

### Drift Correction (Three-Tier)

```go
if abs(drift) > 2.0 {
    // Tier 3: hard seek
    player.Seek(targetPos)
    player.SetSpeed(1.0)
} else if drift < -0.5 {
    // Tier 2a: speed up
    player.SetSpeed(1.05)
} else if drift > 0.5 {
    // Tier 2b: slow down
    player.SetSpeed(0.95)
} else {
    // Tier 1: normal speed
    player.SetSpeed(1.0)
}
```

- On pause sync: seeks to host position immediately, resets speed to 1.0

---

## 6. WebRTC / General P2P Watch Party Patterns

### Typical WebRTC Data Channel Schema

Systems using WebRTC data channels (e.g., via `peerjs` or `simple-peer`) share
a similar message taxonomy:

```typescript
type SyncMessage =
  | { type: "play"; position: number; timestamp: number }
  | { type: "pause"; position: number; timestamp: number }
  | { type: "seek"; position: number; timestamp: number }
  | { type: "state"; position: number; playing: boolean; timestamp: number }
  | { type: "ping"; timestamp: number }
  | { type: "pong"; timestamp: number; serverTimestamp: number }
  | { type: "join"; room: string }
  | { type: "leave" }
  | { type: "chat"; text: string };
```

Common patterns:
- **Host-led**: One peer is "host" and is authoritative; others follow
- **Democratic**: Anyone can control; last-write-wins (often with CRDT clock)
- **CRDT last-write-wins**: `SyncState` carries `timestamp_ms`; highest wins
  (used by `watch-together` Rust CLI)
- **MQTT-based**: Some implementations use MQTT pub/sub for broadcasting state

### Cross-Cutting Patterns

| Concern | Common Approach |
|---------|----------------|
| Echo suppression | `ignoringOnTheFly` ack pattern, or suppress events from same originator within a time window (500ms) |
| Initial sync | Full state snapshot on join (position + playing + wall-clock) |
| Clock sync | NTP-lite: 3 rapid pings, lowest RTT wins, re-measure every 30-60s |
| Drift correction | Speed ramping (0.95x-1.05x) for small drift; hard seek for >2s |
| State machine | Idle / Waiting / Playing / Paused / Buffering |
| Transport | WebSocket (server-mediated) or WebRTC DataChannel (P2P) or raw TCP |
| Message encoding | JSON (most common) or binary (compact, as in PeerWatch) |

---

## Recommendation for PeerLink

Given peerlink's existing architecture (local-first, P2P emphasis, mpv-based),
the following design is recommended:

### State Machine

```
Stopped → Playing ↔ Paused
              ↕
          Buffering
```

No `Waiting` state (unlike Jellyfin) — peerlink nodes join at any time and
seek to the current position immediately.

### Sync Model: Host-Led with Democratic Override

- The room creator is the **host** (source of truth)
- Host broadcasts its playback state every 2s (like PeerWatch's SYNC)
- Any peer can issue play/pause/seek — they send to the host, host echoes
- This prevents conflicts while allowing democratic control

### Message Format

Simple JSON over WebSocket (or WebRTC data channel):

```typescript
// Control commands (any peer → host, host echoes to all)
type SyncCommand =
  | { cmd: "play"; position: f64; wall_clock_ms: i64 }
  | { cmd: "pause"; position: f64; wall_clock_ms: i64 }
  | { cmd: "seek"; position: f64; wall_clock_ms: i64 }
  | { cmd: "speed"; rate: f64 }

// State broadcast (host → all peers, every ~2s)
type StateBroadcast = {
  position: f64;
  playing: bool;
  speed: f64;
  wall_clock_ms: i64;
}

// Clock sync
type Ping = { wall_clock_ms: i64 }
type Pong = { client_wall_clock_ms: i64; server_wall_clock_ms: i64 }
```

### Clock Sync

- NTP-lite: 3 rapid pings on join, lowest RTT sample kept
- Re-measure every 60s
- Each state broadcast from host includes wall clock; peers compute:
  `target_position = state.position + (now_local - state.wall_clock_ms - half_rtt) * speed`

### Drift Correction (borrowed from PeerWatch)

| Drift | Action |
|-------|--------|
| \|drift\| < 0.5s | Normal speed (1.0x) |
| 0.5s < drift < 2s | Speed adjust (0.95x or 1.05x) |
| \|drift\| ≥ 2s | Hard seek + reset speed to 1.0x |
| On pause | Hard seek to target position immediately |

### Echo Suppression

Use the `ignoringOnTheFly` pattern from Syncplay: when a peer issues a
command, it ignores subsequent state broadcasts from the same origin until
the host echoes back the acknowledgment.

### Joining Mid-Stream

1. Peer connects to room
2. Host sends current `StateBroadcast` immediately
3. Peer seeks to `target_position` and sets playing/paused accordingly
4. Peer starts its drift correction loop

### Implementation Order

1. Message types + encoding (serde/JSON with room for binary extension)
2. Host state broadcast loop (2s interval)
3. Peer drift correction (three-tier from PeerWatch)
4. Play/pause/seek command propagation
5. Clock sync (NTP-lite ping/pong)
6. Echo suppression
7. Joining mid-stream
