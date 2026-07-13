# ADR-002: Use `is` for ICE/NAT traversal

**Status:** Accepted
**Date:** 2026-07-10

## Context

Peerlink needs to traverse NATs for P2P connectivity. This requires an ICE agent (RFC 8445) that pairs candidates, runs connectivity checks, and nominates a working path. The ICE agent must compose with Quinn on a shared UDP socket and support STUN-derived server-reflexive candidates and TURN relayed candidates.

## Decision

Use **`is`** — a standalone sans-IO ICE agent extracted from str0m.

- **Sans-IO** — zero internal threads/async; pure state machine driven by `handle_input()` / `poll_output()`
- **Composition** — feeds candidates from STUN queries and TURN allocations; connectivity checks run over the same UDP socket Quinn uses
- **Observability** — exposes candidate state events for evaluation logging

## Alternatives considered

- **librice** — more comprehensive (STUN/TURN built in), but the async IO wrapper clashes with Quinn's socket ownership. Sans-IO core (`rice-proto`) is less documented
- **Full str0m** — overkill; ICE agent extracted for exactly this reason
- **Custom STUN + manual hole-punch** — not defensible; existing libraries save months of work

## Consequences

- STUN server queries are external (thin client, ~20 lines)
- TURN allocations are external (separate TURN client crate, phase 6)
- NIC enumeration is external (standard Rust `std::net` interfaces)
- All discovered candidates fed into `is` via `add_local_candidate()`
