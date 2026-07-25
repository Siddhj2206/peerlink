# iroh-live dependency setup for peerlink

**Source:** https://github.com/n0-computer/iroh-live
**Commit:** `a130f15` (Jul 24, 2026) — tip of `main`
**Previous relevant commit:** `5f95758` (Jul 15, 2026) — updated iroh to 1.0, dropped git deps on moq crates

---

## Workspace structure

The iroh-live repo is a Cargo workspace with these crates:

| Crate | Path | Needed? | Notes |
|-------|------|---------|-------|
| `iroh-live` | `iroh-live/` | **Yes** | High-level API: sessions, rooms, tickets |
| `iroh-moq` | `iroh-moq/` | **Yes** | MoQ transport over iroh/quinn via web-transport-iroh |
| `moq-media` | `moq-media/` | **Yes** | Media pipelines: capture, encode, decode, playout, ABR |
| `rusty-codecs` | `rusty-codecs/` | **Yes** | Codecs: H.264, AV1, Opus + HW accel (VAAPI, VideoToolbox) |
| `rusty-capture` | `rusty-capture/` | Optional | Camera/screen capture (PipeWire, V4L2, AVFoundation) |
| `moq-media-egui` | `moq-media-egui/` | Optional | egui/wgpu video rendering (dev-dep for demos) |
| `moq-media-android` | `moq-media-android/` | No | Android-specific |
| `iroh-live-relay` | `iroh-live-relay/` | No | Relay server binary |
| `iroh-live-cli` | `iroh-live-cli/` | No | CLI tool (irl) |

### Inter-crate dependency graph

```
iroh-live ──→ iroh-moq, moq-media
iroh-moq  ──→ (none; only external: iroh, moq-net, web-transport-iroh)
moq-media ──→ rusty-codecs, rusty-capture (optional)
moq-media-egui ──→ moq-media
rusty-codecs ──→ (none; only external: hang, optional)
rusty-capture ──→ rusty-codecs
```

---

## External dependency versions required

These are the exact versions from `iroh-live/Cargo.toml` `[workspace.dependencies]`:

| Crate | Version | crates.io? |
|-------|---------|------------|
| `iroh` | `1.0.0` | Yes — must pin `1.0.0` exactly, with features `["metrics", "portmapper", "fast-apple-datapath", "tls-aws-lc-rs"]` |
| `iroh-gossip` | `0.101.0` | Yes |
| `iroh-tickets` | `1.0.0` | Yes |
| `iroh-smol-kv` | `0.4.0` | Yes |
| `hang` | `0.19.1` | Yes — crates.io crate `hang` |
| `moq-lite` → `moq-net` | `0.1.11` | Yes — published as `moq-net`, alias `moq-lite` is deprecated shim; use `moq-net = "0.1.11"` or `moq-lite = { package = "moq-net", version = "0.1.11" }` |
| `moq-mux` | `0.5.5` | Yes |
| `moq-relay` | `0.12.2` | Yes (only needed if running a relay) |
| `moq-native` | `0.17.1` | Yes (only needed if running a relay) |
| `web-transport-iroh` | `0.6.0` | Yes |

All MoQ/Hang crates are fetched from crates.io since the `5f95758` update.

---

## Approach: git dependencies + `[patch.crates-io]`

None of the iroh-live workspace crates are published on crates.io (all at `0.1.0` with path deps). The standard approach is:

1. **Add the crates you directly use as git dependencies** with `subdir` pointing to their workspace subdirectory.
2. **List ALL workspace crates in `[patch.crates-io]`** so that any transitive dependency that tries to resolve them by name (e.g., if a path dep were to fallback to registry lookup) is redirected to the same git source.

### Rationale

When Cargo fetches a crate via `git` with `subdir`, it clones the entire repo. Path dependencies between workspace members (e.g., `moq-media = { path = "../moq-media" }`) resolve within the git checkout as long as the source tree is intact. The `[patch.crates-io]` section is a safety net: if any crate in the dependency graph requests one of these names from the registry, the patch ensures they all unify to the same source, preventing duplicate-crate errors.

---

## Exact Cargo.toml entries

```toml
[patch.crates-io]
iroh-live = { git = "https://github.com/n0-computer/iroh-live", subdir = "iroh-live" }
iroh-moq = { git = "https://github.com/n0-computer/iroh-live", subdir = "iroh-moq" }
moq-media = { git = "https://github.com/n0-computer/iroh-live", subdir = "moq-media" }
rusty-codecs = { git = "https://github.com/n0-computer/iroh-live", subdir = "rusty-codecs" }
rusty-capture = { git = "https://github.com/n0-computer/iroh-live", subdir = "rusty-capture" }
moq-media-egui = { git = "https://github.com/n0-computer/iroh-live", subdir = "moq-media-egui" }

[dependencies]
# iroh-live — main crate, with default codecs but without capture
iroh-live = { git = "https://github.com/n0-computer/iroh-live", subdir = "iroh-live", default-features = false }

# External crates iroh-live needs (must match workspace versions)
iroh = { version = "1.0.0", default-features = false, features = ["metrics", "portmapper", "fast-apple-datapath", "tls-aws-lc-rs"] }
iroh-gossip = "0.101.0"
iroh-tickets = "1.0.0"
iroh-smol-kv = { version = "0.4.0", default-features = false }
hang = "0.19.1"
moq-net = "0.1.11"              # or: moq-lite = { package = "moq-net", version = "0.1.11" }
moq-mux = "0.5.5"
web-transport-iroh = "0.6.0"
```

### Minimal subset (just iroh-live, no capture)

If you only need the core iroh-live API without camera/screen capture:

```toml
[patch.crates-io]
iroh-live = { git = "https://github.com/n0-computer/iroh-live", subdir = "iroh-live" }
iroh-moq = { git = "https://github.com/n0-computer/iroh-live", subdir = "iroh-moq" }
moq-media = { git = "https://github.com/n0-computer/iroh-live", subdir = "moq-media" }
rusty-codecs = { git = "https://github.com/n0-computer/iroh-live", subdir = "rusty-codecs" }

[dependencies]
iroh-live = { git = "https://github.com/n0-computer/iroh-live", subdir = "iroh-live", default-features = false }

iroh = { version = "1.0.0", default-features = false, features = ["metrics", "portmapper", "fast-apple-datapath", "tls-aws-lc-rs"] }
iroh-gossip = "0.101.0"
iroh-tickets = "1.0.0"
hang = "0.19.1"
moq-net = "0.1.11"
moq-mux = "0.5.5"
web-transport-iroh = "0.6.0"
```

The external deps (`hang`, `moq-net`, `moq-mux`, `web-transport-iroh`, etc.) may be pulled in transitively, but listing them explicitly pins the versions correctly.

### With all features (capture, HW codecs, GPU rendering)

```toml
[dependencies]
iroh-live = { git = "https://github.com/n0-computer/iroh-live", subdir = "iroh-live" }
# default-features = true enables: h264, opus, capture, wgpu, vaapi, videotoolbox, dmabuf-import, metal-import
```

---

## Pinning to a specific revision

For reproducible builds, pin all entries to the same commit:

```toml
[patch.crates-io]
iroh-live = { git = "https://github.com/n0-computer/iroh-live", subdir = "iroh-live", rev = "a130f15" }
# ... etc
```

---

## Vendoring vs patching tradeoff

| Approach | Pros | Cons |
|----------|------|------|
| **Git + patch** (recommended) | Single-line `Cargo.toml` changes; automatic updates on `rev` bump; no repo bloat | Requires network fetch on build; git dep lag on `cargo update` |
| **Vendor as workspace member** | Fully offline; total control over patches; no patch-section complexity | Must `git subtree` or copy; manual sync with upstream; repo bloat (+~50 crates) |
| **Submodule + path deps** | Works with `cargo publish`; explicit version control | Complex to set up; submodule management overhead |

For now (early prototyping), **git + patch** is recommended. If the dependency stabilises and the project needs `cargo publish`, consider vendoring or publishing the crates.

---

## Verification

After adding entries, run:

```sh
cargo check 2>&1 | head -50
```

Expected: compiles successfully. If you see errors about `iroh-moq` not found or `moq-media` path resolution failures, ensure all workspace crates in the dependency chain (including `iroh-moq`, `moq-media`, `rusty-codecs`) are listed in both `[dependencies]` and `[patch.crates-io]` — or at minimum in `[patch.crates-io]`.

---

## Key files in iroh-live repo

| File | Purpose |
|------|---------|
| `Cargo.toml` (root) | Workspace definition, `[workspace.dependencies]` with all version pins |
| `iroh-live/Cargo.toml` | iroh-live crate deps and feature flags |
| `iroh-moq/Cargo.toml` | MoQ transport layer |
| `moq-media/Cargo.toml` | Media pipeline with codec/capture deps |
| `rusty-codecs/Cargo.toml` | Codec implementations |
| `README.md` | Says: "Downstream users should copy the `[patch.crates-io]` section" |
