# egui + wgpu Rendering Pipeline for Video Frames

> Research for issue #3: How to set up the egui rendering pipeline to display video frames from rusty-codecs.

## Sources

- [moq-media-egui](https://github.com/n0-computer/iroh-live/tree/main/moq-media-egui) — egui integration crate from iroh-live
- [rusty-codecs/src/render.rs](https://github.com/n0-computer/iroh-live/blob/main/rusty-codecs/src/render.rs) — `WgpuVideoRenderer`
- [iroh-live-cli Cargo.toml](https://github.com/n0-computer/iroh-live/blob/main/iroh-live-cli/Cargo.toml) — actual dependency versions in use
- egui docs, eframe docs, egui-wgpu docs, wgpu docs

## 1. Version Compatibility

iroh-live (production reference) uses:

| Crate | Version |
|---|---|
| `egui` | `0.33` |
| `eframe` | `0.33.0` |
| `egui-wgpu` | `0.33.0` |
| `epaint` | `0.33` |
| `wgpu` | `27` |

Latest available as of July 2026:

| Crate | Latest |
|---|---|
| `egui` | `0.35.0` |
| `eframe` | `0.35.0` |
| `egui-wgpu` | `0.35.0` |
| `epaint` | `0.35.0` |
| `wgpu` | `29` |

**Recommendation:** Use `0.35` versions (latest stable). The API is stable and well-documented. If integrating directly with iroh-live/moq-media, you may need to stay at `0.33` / `wgpu 27` to match their dependency tree — but for a standalone project, `0.35` is fine.

Key constraints:
- `egui`, `eframe`, `egui-wgpu`, and `epaint` must be the **same major.minor** version.
- `wgpu` is semver-compatible within the major version (27.x, 29.x).
- Edition `2024` is required (egui 0.33+).

## 2. eframe Desktop App Boilerplate

Minimal app:

```rust
use eframe::egui;

struct VideoApp {
    // frame state here
}

impl eframe::App for VideoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Video will render here");
        });
    }
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0]),
        renderer: eframe::Renderer::Wgpu, // use wgpu backend
        ..Default::default()
    };
    eframe::run_native(
        "PeerLink",
        options,
        Box::new(|_cc| Box::new(VideoApp::default())),
    )
}
```

To access the wgpu `RenderState` (needed for GPU-side texture upload), use `CreationContext::render_state` or `Frame::render_state()`:

```rust
impl eframe::App for VideoApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if let Some(render_state) = frame.render_state() {
            // render_state.device, render_state.queue, render_state.renderer
        }
    }
}
```

## 3. How moq-media-egui Works

The crate provides two levels:

### `FrameView` — low-level frame renderer

- CPU path: converts `VideoFrame` → `egui::ColorImage::from_rgba_unmultiplied()` → `ctx.load_texture()` / `texture.set()`
- wgpu path: delegates to `WgpuVideoRenderer` → registers native wgpu texture with `egui_wgpu::Renderer::register_native_texture()`

### `VideoTrackView` — high-level track renderer

Wraps `FrameView` + `VideoTrack`, handles viewport, frame polling, and returns `egui::Image`.

```rust
let mut view = VideoTrackView::new(&ctx, "video", track);
// in update:
let (image, timestamp) = view.render(&ctx, available_size);
ui.add(image);
```

### `EguiVideoRenderer` — wgpu integration glue

Holds a `WgpuVideoRenderer` + `egui_wgpu::RenderState`. Each frame:

1. Calls `WgpuVideoRenderer::render(frame)` → gets an Rgba8Unorm `TextureView`
2. Calls `renderer.register_native_texture(device, view, filter)` on first frame
3. Calls `renderer.update_egui_texture_from_wgpu_texture(device, view, filter, id)` on subsequent frames
4. Returns the `epaint::TextureId` for use in `egui::Image`

**Conclusion:** You can use `moq-media-egui` directly if you're also using `moq-media`'s `VideoFrame`/`VideoTrack` types. If you have your own frame type, you'll need to replicate the `EguiVideoRenderer` pattern.

## 4. Frame Lifecycle: Decode → GPU → Display

```
┌──────────┐    ┌──────────────┐    ┌──────────────┐    ┌──────────┐
│ Decode   │    │ Upload       │    │ Register     │    │ Display  │
│ (codec)  │───→│ (wgpu/CPU)   │───→│ with egui    │───→│ in UI    │
└──────────┘    └──────────────┘    └──────────────┘    └──────────┘
```

### Path A: CPU-only (no wgpu dependency)

```rust
fn render_frame_cpu(frame: &VideoFrame, texture: &mut egui::TextureHandle) {
    let rgba = frame.rgba_image(); // VideoFrame → RGBA bytes
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [frame.width() as _, frame.height() as _],
        rgba.as_raw(),
    );
    texture.set(image, Default::default());
}
// Display: egui::Image::from_texture(&texture)
```

### Path B: wgpu-accelerated (handles NV12, zero-copy DMA-BUF)

```rust
// Setup (once):
let wgpu_renderer = WgpuVideoRenderer::new(device.clone(), queue.clone());
let mut egui_renderer = EguiVideoRenderer::new(&render_state);

// Each frame:
let texture_view = wgpu_renderer.render(&frame)?;
let (texture_id, dims) = egui_renderer.render(&frame)?;
// Display:
ui.add(egui::Image::from_texture(
    egui::load::SizedTexture::new(texture_id, [dims.0 as _, dims.1 as _]),
));
```

### Path C: Manual wgpu texture upload (for custom frame types)

```rust
// 1. Create an Rgba8Unorm texture
let texture = device.create_texture(&wgpu::TextureDescriptor {
    label: Some("video_frame"),
    size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    format: wgpu::TextureFormat::Rgba8Unorm,
    usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
    ..Default::default()
});

// 2. Write RGBA data
queue.write_texture(
    texture.as_image_copy(),
    rgba_bytes,
    wgpu::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(w * 4),
        rows_per_image: Some(h),
    },
    wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
);

// 3. Register with egui-wgpu renderer
let texture_view = texture.create_view(&Default::default());
let texture_id = egui_wgpu_renderer.register_native_texture(
    &device, &texture_view, wgpu::FilterMode::Linear
);

// 4. Display
ui.add(egui::Image::from_texture(
    egui::load::SizedTexture::new(texture_id, [w as f32, h as f32]),
));
```

## 5. NV12 → RGBA Conversion (GPU Shader)

`WgpuVideoRenderer` handles this with a WGSL shader that renders a fullscreen triangle:

- Y plane uploaded as `R8Unorm` texture
- UV plane uploaded as `RG8Unorm` texture
- Fragment shader: `y = textureSample(y_tex, samp, uv).r; uv = textureSample(uv_tex, samp, uv).rg; r = y + 1.402 * (uv.g - 0.5); g = y - 0.344 * (uv.r - 0.5) - 0.714 * (uv.g - 0.5); b = y + 1.772 * (uv.r - 0.5);`

For `Packed` RGBA frames, it uses `queue.write_texture()` directly — no shader needed.

## 6. Zero-Copy Paths (Linux DMA-BUF / macOS Metal)

- **DMA-BUF (Linux + Vulkan):** The `dmabuf-import` feature creates a wgpu device with `VK_EXT_external_memory_dma_buf` extensions. VAAPI-decoded frames are imported as wgpu textures without copying.
- **Metal (macOS):** Uses `CVMetalTextureCache` to zero-copy import VideoToolbox decoded frames.
- Falls back gracefully: if import fails, downloads NV12 planes and uploads via the CPU NV12 path.

## 7. Key Takeaways for peerlink

1. **Use `moq-media-egui` directly** if adopting `moq-media`'s frame types. It handles everything: CPU fallback, wgpu acceleration, zero-copy import.
2. **Minimal standalone setup:** `eframe 0.35` + `wgpu 29`, with `Renderer::Wgpu`. Access `RenderState` from `Frame` or `CreationContext`.
3. **Frame lifecycle:** `VideoFrame` → `WgpuVideoRenderer::render()` → register texture view with `egui_wgpu::Renderer` → display via `egui::Image`.
4. **CPU-only fallback:** `frame.rgba_image()` → `ColorImage::from_rgba_unmultiplied()` → `TextureHandle::set()` — works without any wgpu dependency.
5. **Cargo.toml dependencies** for a standalone wgpu-rendered video player:

```toml
[dependencies]
eframe = { version = "0.35", default-features = false, features = ["default_fonts", "wgpu"] }
egui-wgpu = "0.35"
wgpu = "29"
pollster = "0.4"
```
