use clap::Parser;
use peerlink::app::PeerlinkApp;
use peerlink::cli::Cli;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    let mode = cli.mode.unwrap_or(peerlink::cli::Mode::Host);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 768.0])
            .with_title("Peerlink"),
        ..Default::default()
    };

    eframe::run_native(
        "peerlink",
        options,
        Box::new(|_cc| Ok(Box::new(PeerlinkApp::new(mode)))),
    )
}
