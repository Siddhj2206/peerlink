# PeerLink

Local-first watch party: one **Host** streams a local H.264/AAC MP4 to a Party of
at most five **Guests**. The desktop app is a Tauri v2 shell with a
React + TypeScript (Vite) webview; the reusable logic lives in the
`peerlink-core` library crate. The webview owns playback — Rust moves bytes and
never decodes video.

This README tracks the **system packages** needed to build and run the app, so we
can confirm them as we test on more machines. Add a section per distro; mark each
one only once an app actually builds and plays a video on it.

## Development environment

The supported setup is an immutable host with everything running inside a
`devbox` toolbox container:

```bash
toolbox run --container devbox <command>   # one-shot
toolbox enter devbox                       # interactive shell
```

## System dependencies

### Fedora 44 — confirmed

Verified on the `devbox` toolbox (Fedora 44, WebKitGTK 2.52.5).

**Toolchain**

- `cargo`, `rustc`
- `nodejs`, `npm` (frontend build)
- `gcc`, `gcc-c++`, `make`, `pkgconf-pkg-config`

**Build — Tauri / webview**

- `webkit2gtk4.1-devel`
- `openssl-devel`
- `libappindicator-gtk3-devel`
- `librsvg2-devel`
- `gtk3-devel`, `libsoup3-devel`, `glib2-devel`, `cairo-devel`, `pango-devel`,
  `gdk-pixbuf2-devel` (pulled in by `webkit2gtk4.1-devel`)

**Build — parked `peerlink-egui` crate (still built by `cargo test --workspace`)**

- `alsa-lib-devel`

**Runtime — app**

- `webkit2gtk4.1`, `gtk3`, `libsoup3`, `librsvg2`, `libappindicator-gtk3`
- `alsa-lib` (audio)

**Runtime — media (GStreamer)**

WebKitGTK's `<video>` is GStreamer-backed and ships **no codecs of its own**, so
every codec the app plays must be installed separately.

- `gstreamer1`
- `gstreamer1-plugins-base`
- `gstreamer1-plugins-good`
- `gstreamer1-plugins-bad-free` (also provides the AAC decoder)
- **`gstreamer1-plugin-openh264`** — the H.264 **video decoder**. Required.
  Without it the MP4 demuxes (duration, scrubber, and play/pause all work) but
  the video is **black**, and WebKitGTK does *not* fire a `<video>` `error`
  event, so the app cannot show a message.

Optional hardware decode:

- `gstreamer1-vaapi` — VA-API; keep the version in step with `gstreamer1`.
- `gstreamer1-libav` (`avdec_h264`) — RPM Fusion only.

> `gstreamer1-plugin-openh264` comes from Fedora's `fedora-cisco-openh264`
> repo, which is enabled by default on Fedora 44.

**Quick check after install**

```bash
gst-inspect-1.0 openh264dec   # prints an element when present
```

If that prints nothing, `<video>` will render black even though playback controls
work.

## Confirmation log

| Distro | Version | Confirmed | Notes |
|---|---|---|---|
| Fedora (toolbox `devbox`) | 44 | ✅ | Needs `gstreamer1-plugin-openh264` for H.264. |
| _(next distro)_ | | | |

## Adding a distro

1. Install the equivalent packages from the lists above.
2. Run `cargo test --workspace`, `npm run test`, and `npm run build`.
3. Launch the app, open an H.264 MP4, and confirm **picture and seeking**.
4. Add a section and a confirmation-log row for the distro.
