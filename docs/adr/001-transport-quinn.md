# ADR-001: Use Quinn (QUIC) for media and control transport

**Status:** Accepted
**Date:** 2026-07-10

## Context

Peerlink needs a transport layer that carries encoded video (loss-tolerant, low-latency) and input/control messages (reliable, ordered) between host and client. The transport must compose with ICE for NAT traversal.

## Decision

Use **Quinn** — a pure-Rust async QUIC implementation.

- **Video on unreliable datagrams** — dropped packets = dropped frame, no head-of-line blocking
- **Input/control on reliable bi-directional streams** — ordered, lossless delivery
- **Composition with ICE** — Quinn operates over a `UdpSocket`; ICE negotiates the remote address; no impedance mismatch

## Alternatives considered

- **TCP + TLS + custom framing** — head-of-line blocking makes video lossy; NAT traversal harder; rejected
- **Full WebRTC via str0m** — overkill; Peerlink doesn't need RTP/RTCP/SRTP/SCTP; the ICE agent was extracted into `is` for exactly this reason
- **Raw RTP/RTCP over UDP** — reinventing QUIC; rejected

## Consequences

- Quinn is async/tokio-based, so the app must use a tokio runtime
- Self-signed certificates for LAN demo; CA-based for WAN
- QUIC runs over UDP — firewall/port-forwarding notes for documentation
