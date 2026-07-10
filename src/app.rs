use crate::cli::Mode;

pub struct PeerlinkApp {
    pub mode: Mode,
}

impl PeerlinkApp {
    pub fn new(mode: Mode) -> Self {
        Self { mode }
    }
}

impl eframe::App for PeerlinkApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("Peerlink");

        ui.horizontal(|ui| {
            ui.label("Mode:");
            let host_selected = self.mode == Mode::Host;
            if ui.selectable_label(host_selected, "Host").clicked() {
                self.mode = Mode::Host;
            }
            if ui.selectable_label(!host_selected, "Client").clicked() {
                self.mode = Mode::Client;
            }
        });

        ui.separator();

        match self.mode {
            Mode::Host => {
                ui.label("Ready to share \u{2014} waiting for connection.");
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
