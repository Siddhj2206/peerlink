# Video pipeline research

## Summary

Major remote-desktop and game-streaming solutions (Parsec, Sunshine/Moonlight, Steam Remote Play) universally converge on a narrow set of encoder parameters for sub-10 ms encode latency: hardware encoding (NVENC/AMF/VAAPI), preset P1 (fastest), `tune ll` or `tune ull`, CBR rate control, single-frame VBV buffer, zero B-frames, zero lookahead, and infinite GOP length. NVENC on modern GPUs delivers 1-3 ms encode latency at 1080p60 H.265; software encoding (libx264 with `ultrafast` + `zerolatency`) is a fallback only. On Linux, capture is the harder problem — PipeWire via xdg-desktop-portal is the standard Wayland path but adds overhead and unreliability, while DRM/KMS direct capture is faster and compositor-agnostic but requires `CAP_SYS_ADMIN`. Sunshine's architecture (C++, FFmpeg-based encoder abstraction, standalone NVENC path) is the closest reference for Peerlink.

## Encoder selection and tuning

### Codec comparison

| Codec | Encode latency (1080p60 HW) | Quality @ 8 Mbps CBR | Client decode support |
|-------|---------------------------|---------------------|----------------------|
| H.264 | ~1-3 ms (NVENC Ada) | Good | Universal |
| H.265/HEVC | ~1-3 ms (NVENC Ada) | Better (~30% savings vs H.264) | Most modern clients |
| AV1 | ~2-4 ms (NVENC Ada) | Best (~40% savings vs H.264) | RTX 40+, Arc, RDNA3+ |

**Recommendation**: Prefer H.265 for LAN (better quality per bit, same latency). Fall back to H.264 for maximum compatibility. Only use AV1 when bandwidth-constrained on WAN — encode latency is slightly higher and client decode support is limited.

### Critical encoder parameters for sub-10 ms latency

All shipping streaming products (Parsec, Sunshine, GeForce NOW, Xbox Cloud Gaming) converge on the same tradeoffs:

1. **No B-frames** — B-frames introduce reordering delay (lookahead into future frames). Disable entirely.
2. **CBR rate control** — Predictable bitrate per frame. VBR can cause latency spikes when complex frames exceed budget.
3. **Single-frame VBV buffer** — Sets `vbvBufferSize = bitrate / framerate`. This caps each frame's size to exactly one frame's worth of bits, eliminating encoder-side buffering delay. This is the single most important parameter for low latency.
4. **Preset P1 (fastest)** — NVENC presets P1-P7 trade quality for speed. P1 is required for sub-5 ms. Sunshine defaults to P1.
5. **Tune `ll` or `ull`** — `tune ll` (low latency) or `tune ull` (ultra low latency) configures the encoder for single-frame operation. NVENC's `ull` tuning disables all frame reordering and lookahead.
6. **Infinite GOP length** — No automatic IDR/keyframe insertion. Keyframes are sent only on client request (error recovery) or scene change. Parsec and Sunshine both use this.
7. **Zero lookahead** — `rc-lookahead = 0`. Lookahead queues frames to optimize bit allocation; adds N frames of latency.
8. **Multipass disabled** — Single-pass encoding. Multipass improves quality but adds latency and GPU VRAM usage. Sunshine defaults to quarter-resolution two-pass (a compromise) but can disable it.
9. **Single reference frame** — `ref = 1`. Multiple reference frames improve compression but increase latency and decoder memory.

### NVENC-specific configuration (from NVIDIA's recommended low-latency settings)

```
-preset p1 -tune ll -rc cbr -multipass disabled
-b:v BITRATE -bufsize BITRATE/FRATE
-g 999999 -b_ref_mode 0 -no-scenecut 1
```

From NVIDIA's Video Codec SDK documentation and latency-sensitive benchmark config:
- `NV_ENC_CONFIG::gopLength = NVENC_INFINITE_GOPLENGTH`
- `NV_ENC_CONFIG::idrPeriod = 0xffffffff`
- `NV_ENC_RC_PARAMS::vbvBufferSize = B/N` (bitrate / framerate)
- `NV_ENC_RC_PARAMS::vbvInitialDelay = B/N`
- `NV_ENC_RC_PARAMS::rateControlMode = NV_ENC_RC_2_PASS_FRAMESIZE_CAP` (strict single-frame cap) or `NV_ENC_PARAMS_RC_CBR`
- `NV_ENC_RC_PARAMS::enableMinQP = 0`
- No B-frames: `NV_ENC_CONFIG_H264::sliceMode = 0`, `numSlice = 0` (default with `tune ll`)

### x264 (software fallback) configuration

```
-preset ultrafast -tune zerolatency
```
Effect:
- `--bframes 0` — no B-frames
- `--force-cfr` — constant frame rate
- `--no-mbtree` — disable macroblock tree rate control
- `--sync-lookahead 0` — no lookahead thread sync
- `--rc-lookahead 0` — no lookahead
- `--sliced-threads` — slice-based threading (lower latency than frame-based)
- `--ref 1` — single reference frame

For VBV control: `--vbv-bufsize <bitrate/fps> --vbv-maxrate <bitrate> --keyint infinite`

### openh264-specific tuning

OpenH264 only supports Constrained Baseline Profile (no B-frames, CABAC optional). Key parameters from `SEncParamExt`:

| Parameter | Recommended value | Notes |
|-----------|------------------|-------|
| `iUsageType` | `SCREEN_CONTENT_REAL_TIME` | Optimizes for screen content (less motion search) |
| `iComplexityMode` | `LOW_COMPLEXITY` | Fastest encoding speed |
| `iRCMode` | `RC_BITRATE_MODE` | CBR-like rate control |
| `bEnableFrameSkip` | `true` | Must be enabled for RC to hit target bitrate |
| `iNumRefFrame` | `1` | Single reference frame |
| `uiIntraPeriod` | `0` or large value | No periodic IDR (only first frame) |
| `iEntropyCodingModeFlag` | `0` (CAVLC) | Faster than CABAC, lower compression |
| `bEnableSceneChangeDetect` | `false` | Avoids extra IDR on scene cut |
| `bEnableDenoise` | `false` | No preprocessing |
| `bEnableBackgroundDetection` | `false` | No preprocessing |
| `bEnableAdaptiveQuant` | `false` | Saves CPU, minimal quality impact |
| `iMultipleThreadIdc` | `0` (auto) or `1` | OpenH264 uses slice-based threading |
| `iMaxQp` | `42` | Quality floor |
| `iMinQp` | `18` | Quality ceiling |
| `iMaxBitrate` | same as `iTargetBitrate` | No peak overshoot |

OpenH264 limitations:
- YUV 4:2:0 only (no 4:2:2 or 4:4:4)
- Constrained Baseline Profile only (no CABAC on baseline, though CAVLC is faster)
- Max resolution 3840×2160 (36864 macroblocks)
- RC requires `bEnableFrameSkip = true` or bitrate will be exceeded
- ~5-15 ms encode latency at 1080p60 (CPU-dependent) — slower than hardware encoders

The benefit of openh264 is its permissive license (BSD-2) and zero external dependencies. For Peerlink's latency goals (<10 ms total encode), openh264 is viable only on fast multi-core CPUs and only as a fallback when no hardware encoder is available.

## Capture methods on Linux

### Comparison

| Method | Compositor support | Permissions | Latency | Zero-copy | Stability |
|--------|-------------------|-------------|---------|-----------|-----------|
| **DRM/KMS (kmsgrab)** | Any (below compositor) | `CAP_SYS_ADMIN` | Lowest | Yes (DMA-BUF) | Best — compositor-agnostic |
| **PipeWire + xdg-desktop-portal** | GNOME, KDE, wlroots | None (user session) | Low-Medium | Yes (DMA-BUF) | Varies — portal backend-dependent |
| **KWin direct screencast** | KDE Plasma only | KWin permission system | Low | Yes (DMA-BUF) | Good — bypasses portal D-Bus |
| **wlr-screencopy** | wlroots only (Sway, Hyprland) | None | Low | Yes (DMA-BUF) | wlroots-specific |
| **X11 (XSHM/XComposite)** | X11 only | None | Medium | No (CPU copy) | Stable but legacy |

### PipeWire portal capture (xdg-desktop-portal)

The standard Wayland path:
1. App requests screencast via D-Bus → `org.freedesktop.portal.ScreenCast`
2. Portal shows user a permission dialog
3. On approval, portal returns a PipeWire file descriptor and node ID
4. App connects to PipeWire and streams DMA-BUF frames

**Performance characteristics**:
- Adds D-Bus round-trip latency on session start
- Frame delivery rate is governed by PipeWire's graph scheduling — the `xdg-desktop-portal-wlr` backend was discovered to drive graph at ~40 fps due to default node rate of `1024/48000`, requiring workarounds to drive at display refresh rate
- Known issues across compositors: black screens on Sway, stuttering on KDE Plasma (KWin bug #469777), frame pacing issues with Nvidia GPUs
- Sunshine PR reports: "doesn't perform as well as KMS capture" on tested systems
- Cursor handling is compositor-dependent and often broken

**When to use**: When running unprivileged (no setcap), when targeting Flatpak/sandbox, when user interaction for permission is acceptable.

### DRM/KMS direct capture

Captures the scanout buffer directly from the kernel, below the compositor:
1. Open `/dev/dri/card0`
2. `drmModeGetFB2()` to get framebuffer handle
3. Import DMA-BUF into EGL/OpenGL via `EGL_EXT_image_dma_buf_import`
4. GPU detiles compressed modifiers (Intel, Nvidia block-linear)
5. Zero-copy: buffer goes directly to encoder

**Performance characteristics**:
- Lowest possible capture latency — reads the actual scanned-out frame
- No user prompt, no D-Bus, no compositor dependency
- Works headless (with vkms or dummy HDMI dongle)
- Works at login screen (before user session)
- Works identically on GNOME, KDE, Sway, Cosmic, TTY
- Immune to compositor updates

**Downsides**:
- Requires `CAP_SYS_ADMIN` (broad capability — best mitigated via a privileged helper binary with seccomp)
- Doesn't capture individual windows (full desktop only)
- Stalls if CRTC becomes inactive (monitor powers off, resolution change)
- Cursor hotspot is approximated from image (VM drivers expose it, bare metal doesn't)
- Multi-GPU buffer ownership is complex

**Sunshine's KMS implementation**: Uses `/dev/dri/card0` with the `drm` library, imports framebuffer into EGL, then passes to encoder. With the XDG portal PR (#4417, merged), KMS was split into a separate `sunshine-kms` service/process. This is the recommended pattern: a minimal privileged helper that does only the KMS read and passes the DMA-BUF via socket to the main unprivileged process.

### Sunshine's capture architecture (reference for Peerlink)

Sunshine's Linux capture evolved through three generations:

1. **KMS/DRM** (original, still default): `src/platform/linux/kmsgrab.cpp` — direct DRM capture, needs `CAP_SYS_ADMIN`
2. **XDG Portal + PipeWire** (v2025, PR #4417): `src/platform/linux/pipewire.cpp` + `portal.cpp` — works unprivileged on GNOME/KDE
3. **KWin direct screencast** (PR #5009, April 2026): `src/platform/linux/kwingrab.cpp` — bypasses portal D-Bus entirely, speaks `zkde_screencast_unstable_v1` directly

All three share a common `pipewire_display_t` base class in `pipewire.cpp`, which manages the PipeWire stream, DMA-BUF import, buffer redundancy detection, and encoder device creation. This is a good architectural pattern.

Key design decisions in Sunshine:
- **Hybrid GPU detection**: Checks for Intel GPU presence to decide whether DMA-BUF from portal can be imported into CUDA. On pure NVIDIA systems, DMA-BUF is imported directly; on Intel+NVIDIA hybrid, CUDA falls back to system memory copy.
- **Buffer redundancy**: Skips frames with identical PTS or corrupted flags (via `SPA_CHUNK_FLAG_CORRUPTED`).
- **Format negotiation**: Queries `EGL_EXT_image_dma_buf_import` for supported fourcc + modifier combinations, negotiates with PipeWire.
- **Memory type abstraction**: Encoder receives frames as `mem_type_e` (system, vaapi, cuda, videotoolbox), conversion happens automatically in the pipeline.

## Resolution and color format

### YUV 4:2:0 is the standard

All major streaming solutions use YUV 4:2:0 (NV12 or I420):
- All hardware encoders accept NV12 natively on all platforms
- 4:2:2 and 4:4:4 offer better color fidelity but: (a) reduce encoder throughput, (b) aren't supported by many decoders, (c) increase bitrate 33-50% for negligible perceptual gain in games
- For screen content with text, 4:4:4 can help, but most solutions accept the tradeoff
- Sunshine supports both 4:2:0 and 4:4:4 (configurable via `chromaSamplingType`)

### Scaling strategy

- Capture at native display resolution
- Scale down to target encode resolution before encoder input (GPU-side with hardware scaling, or CPU-side with sws_scale)
- Sunshine uses the capture → encode device pipeline: the `avcodec_encode_device_t` transforms frames via `sws_scale_frame` for software or `av_hwframe_transfer_data` for hardware
- Integer scaling (e.g., 1440p → 720p) is cleaner than non-integer
- NVIDIA's NVENC can take resolutions up to 8K; scale on GPU with CUDA/NVDEC for zero CPU overhead

### Bit depth

- For HDR streaming, 10-bit color (P010) is used by Sunshine with NVENC/AMF
- For SDR, 8-bit NV12 is standard
- 10-bit adds ~20% bitrate at same quality but improves banding in gradients

## Bitrate and rate control

### Typical bitrates for LAN game streaming

| Resolution | FPS | Minimum | Good | Maximum (LAN) |
|------------|-----|---------|------|---------------|
| 720p | 30 | 2 Mbps | 5 Mbps | 10 Mbps |
| 720p | 60 | 3 Mbps | 8 Mbps | 15 Mbps |
| 1080p | 30 | 4 Mbps | 10 Mbps | 20 Mbps |
| 1080p | 60 | 5 Mbps | 15 Mbps | 30 Mbps |
| 1440p | 60 | 10 Mbps | 25 Mbps | 50 Mbps |
| 4K | 60 | 20 Mbps | 50 Mbps | 100 Mbps |

On LAN (1 Gbps Ethernet), bandwidth is not a constraint. Use higher bitrates for quality. Parsec's default auto bitrate caps at 50 Mbps for 1080p.

### CBR vs VBR tradeoffs

| Mode | Latency | Quality consistency | Use case |
|------|---------|-------------------|----------|
| CBR | Lowest | Variable quality (bitstarved on complex frames) | Real-time streaming, low latency |
| VBR | Low+ (needs lookahead) | More consistent quality | Recording, non-interactive |
| CQP (constant QP) | Lowest | Fixed quality, variable bitrate | Debugging, highest quality when bandwidth is unconstrained |

**Recommendation**: CBR with single-frame VBV for peer-to-peer streaming. The quality variation at CBR is acceptable at LAN bitrates (20+ Mbps). On bandwidth-constrained links, the encoder should lower resolution rather than bitrate (resolution scaling preserves more perceived quality than heavy quantization).

### Rate control algorithm

The critical chain for low-latency rate control:
1. Client sends desired bitrate (based on network capacity)
2. Encoder configures CBR + single-frame VBV: `vbvBufferSize = bitrate / fps`
3. Each frame is encoded with strict bit budget = one frame's worth
4. If network degrades, encoder dynamically adjusts bitrate (NVENC supports in-session bitrate changes via `NvEncReconfigureEncoder`)
5. Sunshine uses `NV_ENC_RC_2_PASS_QUALITY` or CBR with `vbvBufferSize` set dynamically

## Architecture patterns from existing solutions

### Parsec

- **Custom C SDK** (not built on WebRTC or FFmpeg wrappers) — full control over every pipeline stage
- **Zero-copy GPU pipeline**: capture → encode — no system memory round-trip
- **Proprietary BUD protocol**: custom UDP-based transport with DTLS 1.2, tightly coupled with encoder to adjust bitrate on congestion events
- **H.264 primary, H.265 optional** — HEVC increased latency on early consumer hardware per their testing
- **Latency hierarchy**: latency > frame rate > video quality (in that order of priority)
- **1080p60 encoding latency**: 5.8 ms median (NVENC), 15 ms (AMD VCE)
- **240 FPS demo**: 4-8 ms total pipeline latency on LAN with GTX 1070
- **Pipeline**: raw desktop capture → GPU encode → network → GPU decode → render
- **Key insight**: Parsec's entire stack is built for the specific purpose of game streaming, not adapted from general-purpose codecs or transports

### Sunshine

- **C++ with FFmpeg encoder abstraction** (primary) + **standalone NVENC path** (bypasses avcodec for lower level control)
- **Encoder probing**: runtime detection of available HW encoders, sorted by preference: NVENC → AMF → QSV → VAAPI → VideoToolbox → software
- **Codec selection**: negotiates H.264/HEVC/AV1 with Moonlight client based on capabilities
- **NVENC defaults**: P1 preset, quarter-resolution two-pass, single-frame VBV, no weighted prediction, no adaptive quantization (all configurable)
- **Linux capture**: KMS (primary), PipeWire portal (secondary), KWin direct (newest) — all abstracted behind `platf::display_t`
- **Memory management**: separate `avcodec_encode_device_t` for each hwdevice type (cuda, vaapi, videotoolbox)
- **Handles hybrid GPU**: Intel iGPU display + NVIDIA dGPU encode detected and routed through system memory path
- **HDR metadata**: Rec.2020 + SMPTE ST 2084 (HDR10) passed through PipeWire metadata
- **Key architectural feature for Peerlink**: The `platf::capture_e` return type has variants `capture_e::reconfigure` (resolution change), `capture_e::timeout` (missed frame), `capture_e::duplicate` (identical frame), `capture_e::invalid` — this rich status allows the pipeline to react intelligently rather than blindly encoding stale frames.

### Steam Remote Play

- **Multi-path encoder selection**: tries NVENC → VAAPI → libx264 in order
- **Capture methods**: On Linux, uses Desktop OpenGL NV12, Direct3D capture (game thread), or NVFBC/NVIFR (Windows)
- **PipeWire capture** (with `-pipewire` flag): Known to have stale-frame insertion bugs on KDE Wayland (GitHub issue #13348)
- **NVFBC/NVIFR**: Legacy NVIDIA capture APIs (NVFBC deprecated, NVIFR retired). NVFBC captures front buffer directly, NVIFR goes through D3D/OpenGL
- **Steam Link hardware**: Uses hardware decoder on client (RPi, phone, Steam Deck)
- **Latency profile**: ~30-40 ms at 1080p60 on LAN with hardware encoding; software x264 had slightly lower display latency but more CPU usage
- **Valve's key finding** (from forum discussions): NVENC path was *perceptually* more responsive even when latency counters showed higher numbers — because different encoder/decoder paths disagree on "zero ms" start point. This underscores the importance of subjective testing with high-speed cameras, not just instrumented latency.

## Recommendations for Peerlink

### Encoder settings to use

For the primary hardware encoder path (when available):

```
// NVENC (highest priority)
preset = p1
tune = ll         // low latency (or ull for ultra low latency)
rc = cbr
multipass = disabled
vbvBufferSize = bitrate / framerate
vbvInitialDelay = vbvBufferSize
gopLength = INFINITE
idrPeriod = 0xFFFFFFFF
bFrames = 0
rcLookahead = 0

// AMF (fallback)
usage = ultralowlatency
quality = speed
rc = cbr

// VAAPI (Linux Intel/AMD fallback)
// Use VAAPI's low-latency mode with CBR
```

For software fallback (openh264):

```
iUsageType = SCREEN_CONTENT_REAL_TIME
iComplexityMode = LOW_COMPLEXITY
iRCMode = RC_BITRATE_MODE
iNumRefFrame = 1
uiIntraPeriod = 0
iEntropyCodingModeFlag = 0  // CAVLC (faster)
bEnableFrameSkip = true
bEnableSceneChangeDetect = false
bEnableDenoise = false
bEnableBackgroundDetection = false
bEnableAdaptiveQuant = false
iMaxBitrate = iTargetBitrate
```

### Pipeline architecture changes

1. **Adopt Sunshine's encoder abstraction pattern**: a runtime encoder probe that orders NVENC → AMF → VAAPI → openh264, with codec capability negotiation per client session.

2. **Separate capture from encoding**: The capture device should own the buffer and pass it to the encoder via DMA-BUF (GPU) or shared memory (CPU). Sunshine's `platf::display_t` → `platf::encode_device_t` pattern is ideal.

3. **Frame pacing**: Implement capture status feedback (`reconfigure`, `timeout`, `duplicate`, `invalid`) to avoid encoding stale or corrupted frames. Sunshine's buffer redundancy detection (identical PTS + no damage region) is simple and effective.

4. **Rate control feedback loop**: When network congestion is detected (via BUD-like transport), dynamically reduce encoder bitrate. When congestion clears, raise it. Do not change resolution instantly — let bitrate adaptation absorb transient congestion, then scale resolution only if sustained.

5. **Use a privileged helper for KMS capture**: A minimal binary holding `CAP_SYS_ADMIN` that opens `/dev/dri/card*`, calls `drmModeGetFB2()`, and passes the DMA-BUF fd via `SCM_RIGHTS` over a socket. The main process runs unprivileged. This is better than running the whole process with setcap, as Sunshine does.

### Capture strategy

1. **If KMS access is available (setcap configured)**: Use DRM/KMS capture directly. Lowest latency, compositor-agnostic, works headless. This is the gold standard.
2. **If running on KDE Plasma with kwingrab support**: Use KWin direct screencast (bypasses portal D-Bus, lower overhead).
3. **If running unprivileged on GNOME/KDE**: Use PipeWire + xdg-desktop-portal. Accept the latency penalty (~1-2 frames) and the session-init permission dialog.
4. **If running on X11**: Use XSHM capture (SHM get image). Simple, proven, but adds a CPU copy.
5. **Fallback**: Always have a CPU fallback capture (XSHM or PipeWire) for headless or permission-constrained environments.

### Next steps

1. **Benchmark openh264 vs hardware encoders on Peerlink's target hardware** — measure encode latency at 720p60 and 1080p60 for each available encoder. Hardware encoders will likely win by 3-10x.
2. **Implement the encoder probe system** — test NVENC → AMF → VAAPI → openh264 availability at startup.
3. **Prototype KMS capture with a privileged helper** — the helper pattern is proven (libdrmtap, obs-kmscap, Sunshine's kms).
4. **Implement PipeWire portal capture** as the secondary (unprivileged) path, sharing the DMA-BUF infrastructure with KMS.
5. **Test frame pacing** with a 240 fps camera on LAN to measure total pipeline latency (capture → encode → network → decode → render).
6. **Adopt Sunshine's rich capture status enum** (`capture_e::reconfigure`, `timeout`, `duplicate`, `invalid`) for robust pipeline decision-making.

The most important single change for latency is adopting hardware encoding with P1/ultralowlatency settings. Everything else (capture path, frame pacing, rate control) is optimization around that core.
