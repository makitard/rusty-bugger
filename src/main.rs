#![feature(let_chains, offset_of)]

mod debugger;
mod gui;

pub const WINDOW_TITLE: &str = "rusty-bugger";

fn main() {
    gui::app::App::new()
        .show(WINDOW_TITLE)
        .expect("Failed to open egui window");
}
