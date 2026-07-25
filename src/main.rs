mod app;
mod chat;
mod codec;
mod demux;
mod net;
mod room;
mod ticket;

use app::PeerLinkApp;
use eframe::egui;

fn main() -> Result<(), eframe::Error> {
    tracing_subscriber::fmt::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0])
            .with_title("PeerLink"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "PeerLink",
        options,
        Box::new(|_cc| Ok(Box::<PeerLinkApp>::default())),
    )
}
