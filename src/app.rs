use std::ffi::CString;
use std::ptr;

use ffmpeg_next::ffi::*;

use crate::capture::x11::X11Capture;
use crate::capture::wayland::WaylandCapture;
use crate::capture::Capture;
use crate::cli::Mode;
use crate::decode::openh264::Openh264Decoder;
use crate::decode::Decoder;
use crate::encode::openh264::Openh264Encoder;
use crate::encode::vaapi::VaapiEncoder;
use crate::encode::Encoder;

pub struct PeerlinkApp {
    pub mode: Mode,
    host_pipeline: Option<(Box<dyn Capture>, Box<dyn Encoder>, Box<dyn Decoder>)>,
    last_texture: Option<egui::TextureHandle>,
}

impl PeerlinkApp {
    pub fn new(mode: Mode) -> Self {
        let host_pipeline = Self::try_init_pipeline(mode);
        Self {
            mode,
            host_pipeline,
            last_texture: None,
        }
    }

    fn try_init_pipeline(
        mode: Mode,
    ) -> Option<(Box<dyn Capture>, Box<dyn Encoder>, Box<dyn Decoder>)> {
        if mode != Mode::Host {
            return None;
        }
        let capture: Box<dyn Capture> = match WaylandCapture::new() {
            Some(c) => Box::new(c),
            None => Box::new(X11Capture::new()?),
        };
        let encoder: Box<dyn Encoder> = if vaapi_available() {
            Box::new(VaapiEncoder::new().ok()?)
        } else {
            Box::new(Openh264Encoder::new().ok()?)
        };
        let decoder = Box::new(Openh264Decoder::new().ok()?);
        Some((capture, encoder, decoder))
    }

    fn update_host_frame(&mut self, ctx: &egui::Context) {
        let Some((capture, encoder, decoder)) = &mut self.host_pipeline else {
            return;
        };
        let Ok(frame) = capture.capture_frame() else {
            return;
        };
        let Ok(bitstream) = encoder.encode(&frame) else {
            return;
        };
        let Ok(decoded) = decoder.decode(&bitstream) else {
            return;
        };
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [decoded.width as usize, decoded.height as usize],
            &decoded.data,
        );
        self.last_texture = Some(ctx.load_texture("host-frame", image, egui::TextureOptions::LINEAR));
    }
}

impl eframe::App for PeerlinkApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let prev_mode = self.mode;
        let mut next_mode = self.mode;

        ui.heading("Peerlink");

        ui.horizontal(|ui| {
            ui.label("Mode:");
            let host_selected = self.mode == Mode::Host;
            if ui.selectable_label(host_selected, "Host").clicked() {
                next_mode = Mode::Host;
            }
            if ui.selectable_label(!host_selected, "Client").clicked() {
                next_mode = Mode::Client;
            }
        });

        if next_mode != prev_mode {
            self.mode = next_mode;
            self.host_pipeline = Self::try_init_pipeline(next_mode);
            self.last_texture = None;
        }

        ui.separator();

        match self.mode {
            Mode::Host => {
                self.update_host_frame(ui.ctx());
                if let Some(tex) = &self.last_texture {
                    ui.image(tex);
                } else if self.host_pipeline.is_some() {
                    ui.label("Capturing...");
                } else {
                    ui.label("Failed to initialize capture pipeline.");
                }
            }
            Mode::Client => {
                ui.label("Enter host address to connect.");
            }
        }
    }
}

/// Probe whether VAAPI + h264_vaapi are usable at runtime.
/// Creates and immediately destroys a device + codec context.
fn vaapi_available() -> bool {
    unsafe {
        let mut hw_device_ctx: *mut AVBufferRef = ptr::null_mut();
        let ret = av_hwdevice_ctx_create(
            &mut hw_device_ctx as *mut *mut AVBufferRef,
            AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI,
            ptr::null(),
            ptr::null_mut(),
            0,
        );
        if ret < 0 || hw_device_ctx.is_null() {
            return false;
        }

        let name = match CString::new("h264_vaapi") {
            Ok(n) => n,
            Err(_) => {
                av_buffer_unref(&mut hw_device_ctx as *mut *mut AVBufferRef);
                return false;
            }
        };
        let codec = avcodec_find_encoder_by_name(name.as_ptr());
        if codec.is_null() {
            av_buffer_unref(&mut hw_device_ctx as *mut *mut AVBufferRef);
            return false;
        }

        let mut ctx = avcodec_alloc_context3(codec);
        if ctx.is_null() {
            av_buffer_unref(&mut hw_device_ctx as *mut *mut AVBufferRef);
            return false;
        }

        (*ctx).width = 320;
        (*ctx).height = 240;
        (*ctx).time_base = AVRational { num: 1, den: 30 };
        (*ctx).framerate = AVRational { num: 30, den: 1 };
        (*ctx).sample_aspect_ratio = AVRational { num: 1, den: 1 };
        (*ctx).pix_fmt = AVPixelFormat::AV_PIX_FMT_VAAPI;
        (*ctx).codec_type = AVMediaType::AVMEDIA_TYPE_VIDEO;
        (*ctx).bit_rate = 1_000_000;
        (*ctx).gop_size = 30;
        (*ctx).max_b_frames = 0;

        // For VAAPI encoders, avcodec_open2 may need hw_frames_ctx.
        // If the probe fails here, the encoder is still marked unavailable
        // and we'll fall back to OpenH264.
        let mut frames_ctx = av_hwframe_ctx_alloc(hw_device_ctx);
        if !frames_ctx.is_null() {
            let fc = &mut *((*frames_ctx).data as *mut AVHWFramesContext);
            (*fc).format = AVPixelFormat::AV_PIX_FMT_VAAPI;
            (*fc).sw_format = AVPixelFormat::AV_PIX_FMT_NV12;
            (*fc).width = 320;
            (*fc).height = 240;
            (*fc).initial_pool_size = 1;
            if av_hwframe_ctx_init(frames_ctx) >= 0 {
                (*ctx).hw_frames_ctx = av_buffer_ref(frames_ctx);
            }
            av_buffer_unref(&mut frames_ctx as *mut *mut AVBufferRef);
        }

        let ok = avcodec_open2(ctx, codec, ptr::null_mut()) >= 0;
        avcodec_free_context(&mut ctx as *mut *mut AVCodecContext);
        av_buffer_unref(&mut hw_device_ctx as *mut *mut AVBufferRef);
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_creation() {
        let app = PeerlinkApp::new(Mode::Host);
        assert_eq!(app.mode, Mode::Host);

        let app = PeerlinkApp::new(Mode::Client);
        assert_eq!(app.mode, Mode::Client);
    }
}
