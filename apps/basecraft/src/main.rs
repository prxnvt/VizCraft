//! BaseCraft desktop app. Cascade 0 stub: opens an empty window.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

struct App;

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.label("BaseCraft (cascade 0 stub)");
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("BaseCraft").with_inner_size([1440.0, 920.0]),
        ..Default::default()
    };
    eframe::run_native("BaseCraft", options, Box::new(|_cc| Ok(Box::new(App))))
}
