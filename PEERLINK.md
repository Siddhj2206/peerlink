# Peerlink
### A cross-platform peer-to-peer remote desktop tool in Rust
*Project Proposal*

---

## 1. Overview

This project is a remote desktop application in the spirit of AnyDesk or TeamViewer, built from scratch in Rust and targeting both Windows and Linux (including Wayland). The core deliverable is a working screen-sharing and remote-control pipeline over a LAN. The advanced component, and the primary networks-course contribution, is direct peer-to-peer connectivity: two clients establish a connection across NATs using STUN-based candidate gathering and UDP hole punching, falling back to a TURN relay when direct connection isn't possible.

The goal is to keep the system architecture modular so each stage of the pipeline — capture, encoding, transport, NAT traversal, rendering, and input — can be built, tested, and evaluated independently.

## 2. Motivation

Commercial remote-desktop tools are closed source, so the networking internals — particularly how they establish direct connections across NATs — aren't visible or measurable. Building this from the ground up gives direct, hands-on experience with:

- Real-time media transport over an unreliable network (QUIC/UDP)
- NAT traversal: STUN, ICE candidate gathering, UDP hole punching, TURN fallback
- Practical tradeoffs in codec choice, latency, and bandwidth
- Cross-platform systems programming across two very different OS capture/input models

## 3. System Architecture

The system is split into two roles, **host** (shares its screen, receives input) and **client** (views the stream, sends input), implemented from a shared codebase. Data flows through six stages:

- **Capture** — screen frames pulled from the OS (DXGI/XCap on Windows, X11 or Wayland portal on Linux)
- **Encode** — frames compressed with an H264 encoder
- **Transport** — encoded frames and input events sent over a QUIC connection, established either directly (LAN) or via a P2P NAT-traversed path
- **Decode** — client reconstructs frames from the H264 stream
- **Render** — decoded frames displayed in a GPU-accelerated UI
- **Input** — client-side mouse/keyboard events are sent back to the host and injected as synthetic input

```
┌─────────────┐      ┌──────────────┐      ┌───────────┐      ┌─────────────┐
│   Capture   │ ───▶ │   Encode     │ ───▶ │ Transport │ ───▶ │   Decode    │
│ xcap/portal │      │  openh264    │      │  QUIC/ICE │      │  openh264   │
└─────────────┘      └──────────────┘      └───────────┘      └──────┬──────┘
                                                                       ▼
┌─────────────┐      ┌──────────────┐                          ┌───────────┐
│   Input     │ ◀─── │  Input Chan  │ ◀────────────────────────│  Render   │
│   enigo     │      │  (QUIC)      │                          │   GPUI    │
└─────────────┘      └──────────────┘                          └───────────┘
```

## 4. Technology Choices

Wherever possible, well-established crates are used for complex subsystems (codec, transport, ICE/NAT traversal) rather than custom implementations, so effort is concentrated on integration, correctness, and evaluation rather than reinventing protocol-level machinery.

| Concern | Crate | Notes |
|---|---|---|
| Capture (Windows / X11) | `xcap` | DXGI on Windows, XGetImage on X11 — no code branching needed |
| Capture (Wayland) | `ashpd` + `pipewire-rs` | Portal-negotiated screen share, same approach as OBS |
| Video codec | `openh264` | BSD-licensed H264, avoids ffmpeg build complexity |
| Transport | `quinn` | QUIC — built-in encryption, stream multiplexing, congestion control |
| NAT traversal (ICE) | `str0m` (or its extracted standalone agent, `is`) | Sans-IO ICE agent — STUN/TURN candidate gathering + connectivity checks, driven from our own event loop |
| TURN relay | `turn-rs` / minimal custom | Fallback for symmetric NAT |
| Input injection | `enigo` | Cross-platform mouse/keyboard simulation |
| UI | `gpui` + `gpui-component` | Retained-mode rendering; use git dependency (crates.io lags main); Windows now stable (DirectX 11) |
| Serialization | `bincode` / `postcard` | Control and input event messages |

## 5. Platform Support Notes

**Windows:** screen capture and input injection are handled natively by the chosen crates (DXGI via `xcap`, `SendInput` via `enigo`) with no platform-specific code required on our end. GPUI's Windows backend (rendering through DirectX 11) reached stable status alongside Zed's official Windows release, so it's no longer the open risk it once was — worth a quick smoke test early regardless, since GPUI itself is still evolving quickly.

**Linux:** X11 capture is straightforward via `xcap`. Wayland requires negotiating a screen-share session through the `xdg-desktop-portal` and reading frames via PipeWire — more involved than X11, but the custom protocol work is offloaded to the `ashpd` and `pipewire-rs` crates.

## 6. Peer-to-Peer / NAT Traversal (Advanced Component)

A lightweight signaling server allows two peers to exchange connection metadata (ICE candidates) before any direct link exists — this server is not part of the data path, only the discovery step. Once candidates are exchanged, an ICE agent gathers host, server-reflexive (via STUN), and relay (via TURN) candidates, runs connectivity checks, and selects the best working path. Direct connections are attempted first via UDP hole punching; if both peers are behind symmetric NATs, the connection falls back to relaying through a TURN server.

This stage will be instrumented to log which candidate type wins a given connection, and tested across multiple network types (e.g. campus network, home network, mobile hotspot) to report hole-punch success rates as part of the project evaluation.

## 7. Development Plan

| Phase | Deliverable | Notes |
|---|---|---|
| 0 | Smoke-test GPUI + gpui-component hello-world on Windows | Quick sanity check; Windows backend is stable but framework moves fast |
| 1 | Local capture → encode → decode → render pipeline | No networking yet; validates media pipeline |
| 2 | LAN transport over QUIC | Two machines, same network — safety-net working demo |
| 3 | Input injection over control channel | Coordinate scaling between host/client resolutions |
| 4 | Signaling server | Peers exchange connection info before direct link exists |
| 5 | NAT traversal (ICE / STUN) | Core P2P bonus — hole punching, candidate types logged |
| 6 | TURN fallback + evaluation | Symmetric NAT handling; measure hole-punch success rate |
| 7 (stretch) | Polish | Multi-monitor, connection quality indicator, clipboard sync |

*Phases 0–4 form the baseline deliverable — a complete, working LAN remote desktop tool. Phase 5 is the primary bonus component. Phase 6 strengthens the evaluation with measured results. Phase 7 is optional polish if time permits.*

## 8. Evaluation Plan

- Correctness: successful screen sharing and remote input control between two machines, both same-LAN and across separate networks
- NAT traversal success rate across different network/router configurations
- Latency and frame rate measurements under LAN vs. P2P (relayed vs. direct) conditions
- Bandwidth usage comparison between direct and TURN-relayed connections

## 9. Scope Boundaries

To keep the project achievable within the semester, the following are explicitly out of scope: multi-monitor support, audio streaming, file transfer, and clipboard synchronization. These are noted as potential future work rather than deliverables.

## 10. References & Resources

**Transport (QUIC)**
- Quinn docs — https://docs.rs/quinn/latest/quinn/
- Quinn repo & examples — https://github.com/quinn-rs/quinn
- RFC 9000 (QUIC transport protocol) — https://datatracker.ietf.org/doc/html/rfc9000

**NAT traversal / ICE / STUN / TURN**
- str0m (sans-IO WebRTC/ICE) — https://github.com/algesten/str0m
- `is` — standalone sans-IO ICE agent extracted from str0m — https://docs.rs/is/latest/is/
- Firezone engineering blog, *"sans-IO pattern in Rust networking code"* — a clear worked example of composing an ICE agent into a custom protocol (directly analogous to what Phase 5 needs) — https://www.firezone.dev/blog/sans-io
- RFC 8445 (ICE) — https://datatracker.ietf.org/doc/html/rfc8445
- RFC 5389 (STUN) — https://datatracker.ietf.org/doc/html/rfc5389
- RFC 5766 (TURN) — https://datatracker.ietf.org/doc/html/rfc5766

**Screen capture**
- xcap (Windows/X11/Wayland/macOS capture) — https://crates.io/crates/xcap
- ashpd (XDG portal bindings, incl. ScreenCast portal) — https://docs.rs/ashpd/latest/ashpd/desktop/screencast/index.html
- pipewire-rs — https://gitlab.freedesktop.org/pipewire/pipewire-rs

**Video codec**
- openh264 (idiomatic Rust bindings) — https://docs.rs/openh264
- Cisco OpenH264 project — https://github.com/cisco/openh264

**Input injection**
- enigo — https://docs.rs/enigo/latest/enigo/

**UI**
- GPUI Component docs & examples — https://longbridge.github.io/gpui-component/
- gpui-component repo — https://github.com/longbridge/gpui-component
- Zed's GPUI (upstream) — https://github.com/zed-industries/zed

*Note on GPUI Windows support: GPUI's Windows backend has reached stable status (DirectX 11 rendering), shipping alongside Zed's official Windows release. Worth re-confirming current status close to your build date, since the framework is under active development and the maintainers recommend tracking the git branch rather than the crates.io release.*
