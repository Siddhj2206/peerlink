# iroh P2P Video Streaming for a Watch Party Application

> Research conducted 2026-07-25 against primary sources.
> Sources: [iroh-live](https://github.com/n0-computer/iroh-live), [moq.dev](https://doc.moq.dev/), [docs.iroh.computer](https://docs.iroh.computer/protocols/streaming), [crates.io](https://crates.io), [cros-codecs](https://github.com/chromeos/cros-codecs), [moq-dev/moq](https://github.com/moq-dev/moq).

---

## 1. How iroh MoQ Works End-to-End

### Architecture Layers

The stack from bottom to top:

```
┌─────────────────────────────────────────────────────┐
│  Application (watch party UI, sync, chat, etc.)     │
├─────────────────────────────────────────────────────┤
│  iroh-live (high-level API: sessions, rooms, tickets)│
├─────────────────────────────────────────────────────┤
│  moq-media (media pipelines: capture, encode,       │
│    decode, publish, subscribe, playout, ABR)        │
├─────────────────────────────────────────────────────┤
│  rusty-codecs (codec implementations)               │
├─────────────────────────────────────────────────────┤
│  hang (media catalog + container format)            │
├─────────────────────────────────────────────────────┤
│  moq-lite / moq-net (MoQ pub/sub transport)         │
├─────────────────────────────────────────────────────┤
│  web-transport-iroh (WebTransport bridge over iroh) │
├─────────────────────────────────────────────────────┤
│  iroh / quinn (QUIC transport, P2P connections)     │
└─────────────────────────────────────────────────────┘
```

Sources:

- [docs.iroh.computer/protocols/streaming](https://docs.iroh.computer/protocols/streaming)
- [iroh-live README](https://github.com/n0-computer/iroh-live)
- [moq.dev/concept/layer](https://doc.moq.dev/concept/layer/)

### Full Pipeline: Capture → Encode → Publish → Subscribe → Decode → Render

**Capture:**

- `rusty-capture` crate handles cross-platform capture:
  - Linux: PipeWire, V4L2, X11
  - macOS: ScreenCaptureKit, AVFoundation
  - Android: Camera2 via JNI
  - Synthetic: SMPTE test pattern source for testing
- Camera frames are yielded as `VideoFrame` (RGBA) through the `VideoSource` trait

**Encode:**

- `rusty-codecs` implements encoders via the `VideoEncoder` / `AudioEncoder` traits
- Push/pop streaming interface: `push_frame(raw_rgba)` → `pop_packet(encoded_frame)`
- Codecs:
  - H.264 via openh264 (software, default)
  - AV1 via rav1e (software, optional feature)
  - Opus via unsafe-libopus (audio, default)
- Hardware encoders:
  - VAAPI H.264 (Linux)
  - VideoToolbox H.264 (macOS)
  - Android MediaCodec H.264
  - V4L2 H.264 (Raspberry Pi, embedded Linux)

**Publish:**

- `moq-media::publish` module: takes encoded frames, wraps them in the `hang` container format, writes to `moq-lite` tracks
- Each video rendition → independent MoQ track (separate QUIC stream — no head-of-line blocking)
- A `catalog.json` track describes available tracks per the WebCodecs spec
- Adaptive bitrate (ABR) in `moq-media::adaptive` adjusts encoding quality based on bandwidth estimates from MoQ
- `iroh-moq` wraps MoQ transport over iroh's P2P connections via `web-transport-iroh`

**Subscribe:**

- Subscriber connects via `LiveTicket` (contains endpoint address + broadcast name)
- `iroh-live` resolves the ticket, establishes an iroh connection, negotiates MoQ session
- Client reads catalog, selects desired tracks, subscribes

**Decode:**

- `moq-media::subscribe` reads MoQ groups, feeds packets to decoders
- `DynamicVideoDecoder` (in `rusty-codecs/src/codec/dynamic.rs`) auto-selects backend:
  1. VAAPI HW decoder (Linux, if available)
  2. V4L2 HW decoder (Linux, if available)
  3. VideoToolbox HW decoder (macOS, if available)
  4. Android MediaCodec HW decoder
  5. Fallback to software decoder (openh264 / rav1d)
- Playout buffer in `moq-media::playout` handles A/V sync, jitter

**Render:**

- `rusty-codecs::render` module provides wgpu-based GPU rendering
- `moq-media-egui` integrates decoded frames into egui (via wgpu texture upload)
- Zero-copy paths:
  - DMA-BUF import (VAAPI → Vulkan via `dmabuf-import` feature)
  - Metal import (VideoToolbox → Metal via `metal-import` feature)
- GPU backends: Vulkan (Linux), Metal (macOS), D3D12 (Windows, incomplete)

### Code Example

Publisher (from [iroh-live README](https://github.com/n0-computer/iroh-live)):

```rust
let live = Live::from_env().await?.with_router().spawn();
let broadcast = LocalBroadcast::new();

let camera = CameraCapturer::new()?;
broadcast.video().set_source(camera, VideoCodec::H264, [VideoPreset::P720])?;

let audio = AudioBackend::default();
let mic = audio.default_input().await?;
broadcast.audio().set(mic, AudioCodec::Opus, [AudioPreset::Hq])?;

live.publish("hello", &broadcast).await?;
let ticket = LiveTicket::new(live.endpoint().addr(), "hello");
```

Subscriber:

```rust
let live = Live::from_env().await?.spawn();
let sub = live.subscribe(ticket.endpoint, &ticket.broadcast_name).await?;

let audio = AudioBackend::default();
let tracks = sub.media(&audio, Default::default()).await?;

if let Some(mut video) = tracks.video {
    while let Some(frame) = video.next_frame().await {
        // render frame
    }
}
```

---

## 2. What Video Formats/Codecs It Supports

### Codec Support

| Codec             | Status           | Encoder             | Decoder             | Feature Flag |
| ----------------- | ---------------- | ------------------- | ------------------- | ------------ |
| **H.264** (AVC)   | ✅ Default       | openh264 (software) | openh264 (software) | `h264`       |
| **AV1**           | ✅ Optional      | rav1e               | rav1d (git dep)     | `av1`        |
| **Opus**          | ✅ Default       | unsafe-libopus      | unsafe-libopus      | `opus`       |
| **PCM** (raw f32) | ✅ Optional      | passthrough         | passthrough         | `pcm`        |
| H.265 (HEVC)      | ❌ Not supported | —                   | —                   | —            |
| VP8/VP9           | ❌ Not supported | —                   | —                   | —            |
| AAC               | ❌ Not supported | —                   | —                   | —            |

From `rusty-codecs/src/config.rs` (VideoCodec enum):

```rust
pub enum VideoCodec {
    H264(H264),     // profile, constraints, level, inline flag
    AV1(AV1),       // profile, level, tier, bitdepth, chroma subsampling, etc.
    Other(String),  // passthrough for unknown codec strings
}
```

### Container Formats

The `hang` layer ([doc.moq.dev/concept/layer/hang.html](https://doc.moq.dev/concept/layer/hang.html)) supports two container formats:

1. **Legacy container**: lightweight, no frills. Each frame = 62-bit PTS (varint, μs) + codec payload
2. **CMAF container** (fMP4): `moof`+`mdat` boxes per frame. ~100 bytes overhead per frame at 1-frame fragments

The `catalog.json` (or compressed `catalog.json.z` via DEFLATE) describes tracks per WebCodecs spec. Example:

```json
{
  "video": {
    "renditions": {
      "video0": {
        "codec": "avc1.64001f",
        "description": "...",
        "codedWidth": 1280,
        "codedHeight": 720,
        "container": { "kind": "legacy" }
      }
    }
  }
}
```

### Streaming an MP4 from Disk

The CLI supports `--video file:<FILE>` for streaming media files. Under the hood, `moq-media` uses `moq-mux` (fMP4/CMAF demuxer) and `symphonia` (audio demuxer for MP3, WAV, PCM).

There is no dedicated MP4 demuxer crate in iroh-live itself. `moq-mux` (from [moq-dev/moq](https://github.com/moq-dev/moq)) handles CMAF/fMP4 demuxing. For file playback, `symphonia` handles general audio demuxing.

---

## 3. Hardware Acceleration

### Current HW Accel Support

| Platform                 | HW Encode                            | HW Decode               | Zero-Copy Render                         |
| ------------------------ | ------------------------------------ | ----------------------- | ---------------------------------------- |
| **Linux (Intel/AMD)**    | ✅ VAAPI (H.264)                     | ✅ VAAPI (H.264)        | ✅ DMA-BUF import → Vulkan               |
| **Linux (Raspberry Pi)** | ✅ V4L2 (H.264)                      | ✅ V4L2 (H.264)         | ❌ (software render)                     |
| **macOS**                | ✅ VideoToolbox (H.264)              | ✅ VideoToolbox (H.264) | ✅ Metal import                          |
| **Android**              | ✅ MediaCodec (H.264)                | ✅ MediaCodec (H.264)   | ✅ HardwareBuffer→EGL                    |
| **Windows**              | ❌ Software only                     | ❌ Software only        | ❌ (DX12 via wgpu works for render only) |
| **iOS**                  | ✅ VideoToolbox (compiles, untested) | —                       | —                                        |

### VAAPI Details (Linux)

Uses `cros-codecs` crate ([v0.0.6](https://crates.io/crates/cros-codecs), from Google/ChromeOS):

- VAAPI decoder: H.264, H.265, VP8, VP9, AV1
- VAAPI encoder: H.264, VP9, AV1

**Important:** iroh-live only enables the VAAPI H.264 path. While `cros-codecs` supports H.265/VP8/VP9/AV1 VAAPI, iroh-live does not wire them up. The VAAPI H.264 decoder wrapper is at `rusty-codecs/src/codec/vaapi.rs`.

### Missing HW Accel

- **NVENC/NVDEC**: Not supported. No CUDA or NVIDIA-specific codec paths.
- **Vulkan Video**: Not supported. The `wgpu` render path uses Vulkan for rendering, but not for decode.
- **Intel QSV**: Not directly. VAAPI covers Intel GPUs on Linux, but no explicit MSDK/oneVPL path.
- **Windows**: No DirectX Video, D3D11VA, or Media Foundation. Windows is listed as "Missing" in the platform table.

Source: [iroh-live README platform table](https://github.com/n0-computer/iroh-live) and [`rusty-codecs/Cargo.toml`](https://github.com/n0-computer/iroh-live/blob/main/rusty-codecs/Cargo.toml).

---

## 4. Dependency Evaluation

### MP4 Demuxing

| Crate                                                   | Version      | Stars | Last Updated | Notes                                             |
| ------------------------------------------------------- | ------------ | ----- | ------------ | ------------------------------------------------- |
| [shiguredo_mp4](https://crates.io/crates/shiguredo_mp4) | **2026.3.0** | ~151  | 2026-04-03   | Full MP4 read/write. `no_std` compatible. Active. |
| [mp4parse](https://crates.io/crates/mp4parse) (Mozilla) | 0.17.0       | ~447  | 2026-04-14   | Metadata parser only. MPL-2.0. Used in Firefox.   |

**Recommendation:** `shiguredo_mp4` for full read/write MP4 support. `mp4parse` if you only need metadata parsing.

Note: For a watch party, you would typically use `moq-mux` (from moq-dev) which handles fMP4/CMAF for MoQ streaming. For file-on-disk playback, `shiguredo_mp4` is the better choice.

### Video Decoding

| Crate                                               | Version   | Stars  | Last Updated | Notes                                              |
| --------------------------------------------------- | --------- | ------ | ------------ | -------------------------------------------------- |
| [ffmpeg-next](https://crates.io/crates/ffmpeg-next) | **8.1.0** | ~1,938 | 2026-03-18   | FFmpeg wrapper. Maintenance mode. ~5.9M downloads. |
| gstreamer-rs                                        | 0.23      | ~1,600 | Active       | GStreamer bindings. Heavy dependency.              |
| rust-h264 (openh264)                                | 0.9       | —      | Active       | Used by iroh-live. Software only.                  |

**Recommendation:** For a watch party app, **do not use ffmpeg-next** — it adds a massive C dependency and build complexity. The iroh-live approach (openh264 + cros-codecs for HW) is better aligned with a pure-Rust philosophy.

### Video Rendering

| Crate                                             | Version    | Stars   | Last Updated | Notes                                                |
| ------------------------------------------------- | ---------- | ------- | ------------ | ---------------------------------------------------- |
| [wgpu](https://crates.io/crates/wgpu)             | **29.0.4** | ~17,456 | 2026-07-02   | The standard. Vulkan/Metal/D3D12/GL. ~28M downloads. |
| [pixels](https://crates.io/crates/pixels)         | **0.17.2** | ~2,125  | 2026-07-14   | Simple framebuffer. Built on wgpu.                   |
| [softbuffer](https://crates.io/crates/softbuffer) | **0.4.8**  | ~491    | 2025-12-13   | CPU-side framebuffer. No GPU.                        |

**Recommendation:** `wgpu` is the clear winner — cross-platform, well-maintained, used by iroh-live itself. If you just need to push RGBA frames to a window, `pixels` is simpler but adds another abstraction layer. `softbuffer` is for CPU-side rendering only (no scaling, no shaders).

### Summary for a Watch Party App

| Layer         | Recommended Crate        | Why                                  |
| ------------- | ------------------------ | ------------------------------------ |
| P2P transport | iroh + iroh-live         | Native P2P, QUIC, MoQ protocol       |
| MP4 demuxing  | shiguredo_mp4            | Full read/write, actively maintained |
| Video decode  | iroh-live's rusty-codecs | HW accel on all platforms, pure Rust |
| Video render  | wgpu + egui              | GPU-accelerated, cross-platform      |

---

## 5. iroh-live Workspace Specifics

### Workspace Crates (from [Cargo.toml](https://github.com/n0-computer/iroh-live/blob/main/Cargo.toml))

| Crate               | Description                                      | Published on crates.io? |
| ------------------- | ------------------------------------------------ | ----------------------- |
| `iroh-live`         | High-level API: sessions, rooms, tickets         | ❌ No                   |
| `iroh-live-relay`   | Relay server for browser WebTransport            | ❌ No                   |
| `iroh-live-cli`     | `irl` CLI tool                                   | ❌ No                   |
| `iroh-moq`          | MoQ transport over iroh via `web-transport-iroh` | ❌ No                   |
| `moq-media`         | Media pipelines (no iroh dependency)             | ❌ No                   |
| `moq-media-egui`    | egui integration for video rendering             | ❌ No                   |
| `moq-media-dioxus`  | dioxus-native rendering (prototype)              | ❌ No                   |
| `moq-media-android` | Android camera/EGL/JNI                           | ❌ No                   |
| `rusty-codecs`      | Codec implementations, HW accel, wgpu render     | ❌ No                   |
| `rusty-capture`     | Cross-platform screen/camera capture             | ❌ No                   |
| `demos/*`           | Demo applications                                | ❌ No                   |

**None of the iroh-live workspace crates are published on crates.io.** They are path-only dependencies.

### External Dependencies (published on crates.io)

| Dependency                | Version Used | Notes                                        |
| ------------------------- | ------------ | -------------------------------------------- |
| `iroh`                    | 1.0.0        | Core P2P library, just hit 1.0               |
| `iroh-gossip`             | 0.101.0      | For room/gossip features                     |
| `iroh-tickets`            | 1.0.0        | Ticket serialization                         |
| `iroh-smol-kv`            | 0.4.0        | Key-value store for protocols                |
| `moq-net` (as `moq-lite`) | 0.1.11       | MoQ pub/sub transport. Formerly `moq-lite`.  |
| `moq-mux`                 | 0.5.5        | fMP4/CMAF muxer/demuxer                      |
| `moq-relay`               | 0.12.2       | MoQ relay server                             |
| `moq-native`              | 0.17.1       | Native MoQ helper (with `iroh` feature)      |
| `hang`                    | 0.19.1       | Media format layer (catalog + container)     |
| `web-transport-iroh`      | 0.6.0        | WebTransport over iroh                       |
| `cros-codecs`             | 0.0.6        | Linux VAAPI/V4L2 HW codecs (Google/ChromeOS) |

### Git Dependency Setup

Per the recent commit [`5f95758`](https://github.com/n0-computer/iroh-live/commit/5f95758fcd1450e443a9134c9d9342bcc3957b85):

> "We no longer need git dependencies on moq crates."

All dependencies are now published on crates.io (no git dependencies). However, `rav1d` (AV1 decoder) is still a git dependency:

```toml
rav1d = { git = "https://github.com/memorysafety/rav1d", ... }
```

### How to Use iroh-live as a Downstream Consumer

The README states: "Right now iroh-live uses unreleased versions of several crates. Downstream users should copy the `[patch.crates-io]` section of Cargo.toml for now."

You must either:

1. Vendor the entire `iroh-live` workspace and use path dependencies, or
2. Use `[patch.crates-io]` to redirect the unpublished crates to git/path sources, or
3. Wait for crates to be published (planned but no timeline)

---

## 6. Alternative Approaches

### Approach A: iroh-live with MoQ (Recommended for real-time)

**Pros:**

- Full pipeline: capture → encode → publish → subscribe → decode → render
- Sub-second latency (MoQ optimized for live)
- P2P by default, relay to browser via WebTransport
- Adaptive bitrate built in
- Multi-track: separate video/audio streams, simulcast
- Room support (multi-party, early stage)
- Pure Rust, no C dependencies for basic codecs

**Cons:**

- Workspace crates not published (git/path only)
- Early tech preview (the label says: "Early tech preview")
- No H.265/VP9 support
- Windows HW accel missing
- Room feature "functional but lightly tested"
- iroh-live is still immature (~114 stars, 558 commits)

### Approach B: iroh-roq (RTP over QUIC)

[iroh-roq](https://crates.io/crates/iroh-roq) implements [RTP over QUIC](https://datatracker.ietf.org/doc/draft-ietf-avtcore-rtp-over-quic/) as an iroh protocol. Currently at v0.1.0.

**Pros:**

- Published on crates.io
- Simpler than MoQ
- RTP ecosystem compatibility
- Used by the `callme` demo for audio-only streaming

**Cons:**

- No video pipeline at all — just RTP transport. You'd need to build everything else.
- v0.1.0 — extremely early
- No media format, no codec integration, no render pipeline
- Audio-only in practice (the callme demo)

**Verdict:** Not suitable for a watch party app. You'd have to build the entire video pipeline from scratch.

### Approach C: Raw iroh Blobs for HLS Segments

Instead of live streaming, you could use [iroh-blobs](https://docs.rs/iroh-blobs) (the blob store) to distribute pre-encoded HLS segments (`.ts` or fMP4 segments + `.m3u8` playlists).

**Pros:**

- Very simple: store files in iroh, retrieve by hash
- Use any codec/container (HLS is just files)
- No latency requirements (watch party is near-sync, not real-time)
- iroh-blobs is mature and published on crates.io
- Can use ffmpeg or any tool to encode

**Cons:**

- No live streaming — all content must be pre-encoded
- No sub-second seek/join; must download segments
- No adaptive bitrate built in (you'd implement ABR by switching between variant playlists)
- Requires segmenting content and distributing via iroh's sync protocol
- Every peer must download segments (no relay transcoding)
- Synchronization between peers is manual

**Verdict:** Interesting for a "download party" (all peers sync the same blob collection and play locally), but harder to synchronize viewing position across peers. Works well for pre-recorded content.

### Comparison Table

| Feature             | iroh-live (MoQ)         | iroh-roq (RoQ)          | iroh-blobs (HLS)        |
| ------------------- | ----------------------- | ----------------------- | ----------------------- |
| Live streaming      | ✅                      | Requires building       | ❌ Pre-recorded only    |
| Latency             | <1s                     | <100ms (theoretical)    | 5-30s (HLS segment)     |
| P2P                 | ✅                      | ✅                      | ✅                      |
| Browser support     | ✅ via relay            | ❌                      | ✅ via HLS.js           |
| Codec support       | H.264, AV1, Opus        | None (raw RTP)          | Any (via ffmpeg)        |
| HW accel            | ✅ VAAPI/VTB/Mediacodec | N/A                     | N/A                     |
| ABR built-in        | ✅                      | ❌                      | Partial (HLS variants)  |
| Room/group sync     | ✅ rooms (early)        | ❌                      | ❌ (manual sync)        |
| crates.io published | ❌ (path deps)          | ✅ (v0.1.0)             | ✅ (mature)             |
| Maturity            | Early tech preview      | Very early              | Stable                  |
| Complexity          | High (full pipeline)    | Medium (transport only) | Low (file distribution) |

### Recommendation for a Watch Party

**For real-time sync (watching together):** Use **iroh-live** despite its immaturity. The watch party use case aligns well with its design: P2P, low latency, multi-track, room support. The main cost is the git dependency setup for unpublished crates.

**For pre-recorded content:** Consider a hybrid approach — encode to HLS/fMP4 using standard tools, distribute segments via **iroh-blobs**, and use **iroh-gossip** for synchronization messages ("play at timestamp X"). This avoids the complexity of the live encoding pipeline entirely.

**The iroh-roq path** is not recommended — it's too raw and lacks video infrastructure.

---

## Key Findings Summary

1. **iroh-live is the right foundation** for a P2P watch party, but it's early. Expect API churn, missing Windows HW accel, and unpublished crates.

2. **Codec support is limited** to H.264 (software + HW) and AV1 (software only). No H.265/VP9. For a watch party, H.264 is sufficient — everything decodes it.

3. **Linux HW acceleration works well** through VAAPI (via cros-codecs from Google). Zero-copy DMA-BUF import to Vulkan is a highlight. Windows is the weak link.

4. **The workspace is not published** — you must vendor it or use `[patch.crates-io]`. The README acknowledges this. The latest commit (2026-07-15) updated to iroh 1.0 and removed git deps on MoQ crates, which is progress.

5. **The moq-dev ecosystem is healthy** — `moq-net`, `hang`, `moq-mux` are all published and actively maintained by both Cloudflare and independent contributors.

6. **For a simpler path with pre-recorded content**, iroh-blobs + HLS is viable and avoids the live encoding complexity.
