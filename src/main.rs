// Suppress the console window that would otherwise appear behind the GUI on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod build_info {
    include!(concat!(env!("OUT_DIR"), "/build_info.rs"));
}

mod app;
mod audio;
mod config;
mod metadata;
mod pipe;
mod tagging;
mod track;
mod ui;
mod workers;

use std::sync::Arc;

use app::VriprApp;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let rt = Arc::new(tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime"));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("VRipr - Master Vinyl Rippage")
            .with_inner_size([1100.0, 680.0])
            .with_min_inner_size([900.0, 500.0]),
        ..Default::default()
    };

    let rt_clone = rt.clone();
    eframe::run_native(
        "VRipr - Master Vinyl Rippage",
        options,
        Box::new(move |cc| Ok(Box::new(VriprApp::new(cc, rt_clone)))),
    )
}
