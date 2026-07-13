use crate::capture::x11::X11Capture;
use crate::capture::Capture;
use crate::cli::Mode;
use crate::decode::openh264::Openh264Decoder;
use crate::decode::Decoder;
use crate::encode::openh264::Openh264Encoder;
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
        let capture = Box::new(X11Capture::new()?);
        let encoder = Box::new(Openh264Encoder::new().ok()?);
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
