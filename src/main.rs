/*
 *  main.rs
 *
 *  vripr - The vinyl viper for perfect rippage - Audacity vinyl ripping helper
 *	(c) 2025-26 Stuart Hunter
 *
 *	TODO:
 *
 * MIT License
 * 
 * Copyright (c) 2026 VRipr Contributors
 * 
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 * 
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 * 
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 *
 */

// Suppress the console window that would otherwise appear behind the GUI on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

#[allow(dead_code)]
#[allow(unused_imports)]

mod build_info {
    include!(concat!(env!("OUT_DIR"), "/build_info.rs"));
}

mod app;
mod audio;
mod config;
mod fonts;
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
