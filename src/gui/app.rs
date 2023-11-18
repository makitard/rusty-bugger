use std::error::Error;

use eframe::egui;

use super::disassembly_view::DisassemblyView;
use crate::debugger::{self, Debugee};
use crate::WINDOW_TITLE;

macro_rules! instruction {
    ($ui:ident, $ctx:ident, $name:ident) => {
        $ui.label(format!("{}: {:#x}", stringify!($name).to_uppercase(), $ctx.$name));
    };
}

pub struct App {
    debugee: Option<Debugee>,
    disassembly_view: DisassemblyView,
    pub status: String,
}

impl App {
    pub fn new() -> Self {
        Self {
            debugee: None,
            disassembly_view: DisassemblyView::new(),
            status: String::from("Idle"),
        }
    }

    pub fn show(self, title: &'static str) -> Result<(), Box<dyn Error>> {
        let native_options = eframe::NativeOptions {
            initial_window_size: Some(egui::vec2(1024.0, 576.0)),
            ..Default::default()
        };

        eframe::run_native(title, native_options, Box::new(move |_| Box::new(self)))?;

        Ok(())
    }

    fn open_file(&mut self) -> Result<(), Box<dyn Error>> {
        let file = rfd::FileDialog::new().set_title("Open binary").pick_file();

        if let Some(file) = file {
            if rfd::MessageDialog::new()
                .set_title(WINDOW_TITLE)
                .set_level(rfd::MessageLevel::Warning)
                .set_description(&format!("Are you sure you want to open {file:?}"))
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
            {
                self.debugee = Some(Debugee::new(file)?);
            }
        }

        Ok(())
    }

    fn handle_status(&mut self, status: i32) {
        let debugee = self.debugee.as_mut().unwrap();
        debugee.update_context();
        self.disassembly_view.set_rip(debugee.context().rip);
        //self.disassembly_view.update_cache();

        let signal = libc::WSTOPSIG(status);
        //:P
        let signal_kind = if signal < 32 && signal > 0 {
            unsafe { *(&signal as *const i32 as *const debugger::Signal) }
        } else {
            debugger::Signal::UNKNOWN
        };
        self.status = format!("Received stop signal {:?} ({})", signal_kind, signal);

        if libc::WIFSTOPPED(status) {
            debugee.stopped = true;
            if libc::WSTOPSIG(status) == libc::SIGTRAP {
                let rip = debugee.context().rip - 1;

                if let Some(bp) = debugee.breakpoint_at_address(rip) && !bp.hardware() {
                    let new_rip = rip + bp.size() as u64;
                    debugee.set_rip(new_rip);
                }
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let open_shortcut = egui::KeyboardShortcut::new(egui::Modifiers::CTRL, egui::Key::O);

        if let Some(debugee) = self.debugee.as_mut() {
            if let Ok(status) = debugee.waitpid_communication.1.try_recv() {
                self.handle_status(status);
            }
        }

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    ui.set_min_width(220.0);
                    ui.style_mut().wrap = Some(false);

                    if ui
                        .add(
                            egui::Button::new("Open")
                                .shortcut_text(ui.ctx().format_shortcut(&open_shortcut)),
                        )
                        .clicked()
                        || ui.input_mut(|i| i.consume_shortcut(&open_shortcut))
                    {
                        if let Err(error) = self.open_file() {
                            rfd::MessageDialog::new()
                                .set_title(WINDOW_TITLE)
                                .set_description(&format!("Error while opening file: {error}"))
                                .set_level(rfd::MessageLevel::Error)
                                .show();
                        }
                        ui.close_menu();
                    }
                });
            });
        });

        egui::TopBottomPanel::bottom("status")
            .exact_height(24.0)
            .show(ctx, |ui| {
                ui.label(&self.status);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_enabled_ui(self.debugee.is_some(), |ui| {
                egui::TopBottomPanel::top("control_bar").show_inside(ui, |ui| {
                    egui::menu::bar(ui, |ui| {
                        if ui.button("⏹").clicked() {
                            if let Some(debugee) = self.debugee.as_mut() {
                                debugee.kill();
                            }

                            self.debugee = None;
                        }

                        if ui.button("▶").clicked() {
                            if let Some(debugee) = self.debugee.as_mut() {
                                if debugee.stopped {
                                    debugee.r#continue();
                                    self.status = String::from("Resumed");
                                }
                            }
                        }

                        if ui.button("⏸").clicked() {
                            if let Some(debugee) = self.debugee.as_mut() {
                                if !debugee.stopped {
                                    debugee.stop();
                                    self.status = String::from("Stopped");
                                }
                            }
                        }

                        ui.separator();

                        if ui.button("⎘").clicked() {
                            if let Some(debugee) = self.debugee.as_mut() {
                                if debugee.stopped {
                                    debugee.single_step();
                                } else {
                                    self.status = String::from("Can't single step while unstopped");
                                    //unstopped? unpaused? running? whatever, i'll use unstopped for consistency reasons but it rly doesn't make sense
                                }
                            }
                        }
                    });
                });

                egui::TopBottomPanel::bottom("data").show_inside(ui, |ui| {
                    ui.label("data");
                });

                egui::SidePanel::right("registers")
                    .min_width(175.0)
                    .show_inside(ui, |ui| {
                        if let Some(debugee) = self.debugee.as_ref() {
                            //TODO: add editing (need a custom widget probably)
                            let ctx = debugee.context();

                            instruction!(ui, ctx, rsp);

                            ui.separator();

                            instruction!(ui, ctx, rax);
                            instruction!(ui, ctx, rbx);
                            instruction!(ui, ctx, rcx);
                            instruction!(ui, ctx, rdx);

                            ui.separator();

                            instruction!(ui, ctx, r8);
                            instruction!(ui, ctx, r9);
                            instruction!(ui, ctx, r10);
                            instruction!(ui, ctx, r11);
                            instruction!(ui, ctx, r12);
                            instruction!(ui, ctx, r13);
                            instruction!(ui, ctx, r14);
                            instruction!(ui, ctx, r15);

                            ui.separator();

                            instruction!(ui, ctx, rsi);
                            instruction!(ui, ctx, rdi);

                            ui.separator();

                            instruction!(ui, ctx, rbp);
                            instruction!(ui, ctx, rsp);

                            ui.label(format!("EFLAGS: {:#b}", ctx.eflags));
                        }
                    });

                egui::CentralPanel::default().show_inside(ui, |ui| {
                    if let Some(debugee) = self.debugee.as_mut() {
                        self.disassembly_view.show(ui, debugee);
                    } else {
                        ui.label("Please load a binary to view its disassembly");
                    }
                });
            });
        });
    }
}
