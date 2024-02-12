use std::error::Error;

use eframe::egui;

use super::disassembly_view::DisassemblyView;
use super::hex_view::HexView;
use crate::debugger::{self, Debugee};
use crate::gui::widgets;
use crate::WINDOW_TITLE;

const REGISTER_REFRESH_RATE: f32 = 1.0;

macro_rules! instruction {
    ($self:ident, $ui:ident, $debugee:ident, $name:ident, $dirty:ident) => {
        $ui.horizontal(|ui| {
            ui.add_sized(
                egui::vec2(32.0, 4.0),
                egui::Label::new(format!("{}:", stringify!($name).to_uppercase())),
            );

            let mut buffer = format!("0x{:016x}", $debugee.context().$name);
            let mut modified = false;

            ui.add(widgets::editable_label(
                &mut buffer,
                &mut modified,
                $dirty,
                18,
                135.0,
                stringify!($name),
            ));

            if modified {
                if let Some(hex_string) = buffer.split('x').last() {
                    if let Ok(x) = u64::from_str_radix(&hex_string, 16) {
                        $debugee.write_user(
                            std::mem::offset_of!(libc::user, regs)
                                + std::mem::offset_of!(libc::user_regs_struct, $name),
                            x,
                        );
                        $debugee.update_context();
                        $self.regs_dirty = true;
                    } else {
                        $self.status = format!(
                            "Invalid value for register {}",
                            stringify!($name).to_uppercase()
                        );
                    }
                }
            }
        });
    };
}

#[derive(Clone)]
struct Process {
    pid: u32,
    exe_path: String,
}

pub struct App {
    debugee: Option<Debugee>,
    disassembly_view: DisassemblyView,
    hex_view: HexView,
    pub status: String,

    since_reg_refresh: std::time::SystemTime,
    regs_dirty: bool,

    render_attach_modal: bool,
    process_list: Vec<Process>,
}

impl App {
    pub fn new() -> Self {
        Self {
            debugee: None,
            disassembly_view: DisassemblyView::new(),
            hex_view: HexView::new(),
            status: String::from("Idle"),

            since_reg_refresh: std::time::SystemTime::UNIX_EPOCH,
            regs_dirty: false,

            render_attach_modal: false,
            process_list: Vec::new(),
        }
    }

    pub fn show(self, title: &'static str) -> Result<(), Box<dyn Error>> {
        let native_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size(egui::vec2(1296.0, 729.0)),
            ..Default::default()
        };

        eframe::run_native(title, native_options, Box::new(move |_| Box::new(self)))?;

        Ok(())
    }

    fn open_file(&mut self, ctx: &egui::Context) -> Result<(), Box<dyn Error>> {
        let file = rfd::FileDialog::new().set_title("Open binary").pick_file();

        if let Some(ref file) = file {
            if rfd::MessageDialog::new()
                .set_title(WINDOW_TITLE)
                .set_level(rfd::MessageLevel::Warning)
                .set_description(&format!("Are you sure you want to open {file:?}"))
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
                != rfd::MessageDialogResult::No
            {
                let child_process = std::process::Command::new(file).spawn()?;

                self.debugee = Some(Debugee::new(child_process.id())?);

                ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                    "{WINDOW_TITLE} - {}",
                    file.file_name().unwrap().to_str().unwrap()
                )));
            }
        }

        Ok(())
    }

    fn refresh_process_list(&mut self) -> Result<(), Box<dyn Error>> {
        self.process_list = std::fs::read_dir("/proc/")?
            .into_iter()
            .flatten()
            .filter(|x| x.path().is_dir())
            .map(|x| x.file_name().to_string_lossy().to_string())
            .flat_map(|x| {
                Ok::<_, Box<dyn Error>>(Process {
                    pid: x.parse::<u32>()?,
                    exe_path: std::fs::read_link(format!("/proc/{x}/exe"))?
                        .to_string_lossy()
                        .to_string(),
                })
            })
            .filter(|x| x.pid != std::process::id())
            .collect();
        Ok(())
    }

    fn attach_to_process(
        &mut self,
        ctx: &egui::Context,
        process: &Process,
    ) -> Result<(), Box<dyn Error>> {
        self.debugee = Some(Debugee::new(process.pid)?);

        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
            "{WINDOW_TITLE} - {}",
            process.exe_path
        )));

        Ok(())
    }

    fn handle_status(&mut self, status: i32) {
        let debugee = self.debugee.as_mut().unwrap();
        debugee.update_context();
        self.disassembly_view.set_rip(debugee.context().rip);
        self.disassembly_view.refresh_cache(&debugee);

        self.hex_view.update_cache(debugee);
        self.hex_view.clean_cache();

        if libc::WIFEXITED(status) {
            self.status = format!("Process exited with code {}", libc::WEXITSTATUS(status));
            debugee.stopped = true;
            return;
        }

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
            self.regs_dirty = true;

            if libc::WSTOPSIG(status) == libc::SIGTRAP {
                let rip = debugee.context().rip - 1;

                //TODO fix breakpoints completely, they broke again xddddddddddddddddddddddddddddddddddd
                if let Some(bp) = debugee.breakpoint_at_address(rip)
                    && !bp.hardware()
                {
                    let new_rip = rip + bp.size() as u64;
                    debugee.set_rip(new_rip);
                }
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(debugee) = &mut self.debugee && debugee.stopped {
            if std::time::SystemTime::now().duration_since(self.since_reg_refresh).map(|x| x > std::time::Duration::from_secs_f32(1.0 / REGISTER_REFRESH_RATE)).unwrap_or(false) {
                self.regs_dirty = true;
                self.since_reg_refresh = std::time::SystemTime::now();
            }

            if self.regs_dirty {
                debugee.update_context();
            }
        }

        if self.render_attach_modal {
            let modal =
                egui_modal::Modal::new(ctx, "attach_modal").with_close_on_outside_click(true);
            modal.open();

            let mut attach_process = None;

            modal.show(|ui| {
                modal.title(ui, "Attach");

                modal.frame(ui, |ui| {
                    egui::ScrollArea::new([false, true])
                        .max_height(500.0)
                        .show(ui, |ui| {
                            egui::Grid::new("process_list_grid")
                                .num_columns(3)
                                .show(ui, |ui| {
                                    ui.label("");
                                    ui.label("PID");
                                    ui.label("Executable");
                                    ui.end_row();

                                    for process in &self.process_list {
                                        if ui.button("💉").clicked() {
                                            attach_process = Some(process.clone());
                                        }

                                        ui.label(process.pid.to_string());
                                        ui.label(&process.exe_path);
                                        ui.end_row();
                                    }
                                });
                        });
                });

                modal.buttons(ui, |ui| {
                    if modal.button(ui, "Refresh").clicked() {
                        let _ = self.refresh_process_list();
                    }

                    if modal.suggested_button(ui, "Cancel").clicked()
                        || modal.was_outside_clicked()
                        || attach_process.is_some()
                    {
                        modal.close();
                        self.render_attach_modal = false;
                    }
                });
            });

            if let Some(process) = attach_process {
                if let Err(error) = self.attach_to_process(ctx, &process) {
                    rfd::MessageDialog::new()
                        .set_title(WINDOW_TITLE)
                        .set_description(&format!("Error while attaching to process: {error}"))
                        .set_level(rfd::MessageLevel::Error)
                        .show();
                }
            }
        }

        let open_shortcut = egui::KeyboardShortcut::new(egui::Modifiers::CTRL, egui::Key::O);
        let attach_shortcut = egui::KeyboardShortcut::new(egui::Modifiers::CTRL, egui::Key::A);

        if let Some(debugee) = self.debugee.as_mut() {
            if let Ok(status) = debugee.waitpid_communication.1.try_recv() {
                self.handle_status(status);
            }
        }

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            if !ui.ctx().wants_keyboard_input()
                && ui.input_mut(|i| i.consume_shortcut(&open_shortcut))
            {
                if let Err(error) = self.open_file(ctx) {
                    rfd::MessageDialog::new()
                        .set_title(WINDOW_TITLE)
                        .set_description(&format!("Error while opening file: {error}"))
                        .set_level(rfd::MessageLevel::Error)
                        .show();
                }
            }

            if !ui.ctx().wants_keyboard_input()
                && ui.input_mut(|i| i.consume_shortcut(&attach_shortcut))
            {
                //TODO handle
                let _ = self.refresh_process_list();
                self.render_attach_modal = true;
            }

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
                    {
                        if let Err(error) = self.open_file(ctx) {
                            rfd::MessageDialog::new()
                                .set_title(WINDOW_TITLE)
                                .set_description(&format!("Error while opening file: {error}"))
                                .set_level(rfd::MessageLevel::Error)
                                .show();
                        }
                        ui.close_menu();
                    }

                    if ui
                        .add(
                            egui::Button::new("Attach")
                                .shortcut_text(ui.ctx().format_shortcut(&attach_shortcut)),
                        )
                        .clicked()
                    {
                        //TODO handle
                        let _ = self.refresh_process_list();
                        self.render_attach_modal = true;
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
                        //detach
                        //TODO: icon
                        if ui.button("DETACH").clicked() {
                            if let Some(debugee) = self.debugee.as_mut() {
                                debugee.detach();
                            }

                            self.debugee = None;

                            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                                WINDOW_TITLE.to_owned(),
                            ));

                            self.hex_view.purge_cache();
                            self.disassembly_view.purge_cache();
                        }

                        ui.separator();

                        if ui.button("⏹").clicked() {
                            if let Some(debugee) = self.debugee.as_mut() {
                                debugee.kill();
                            }

                            self.debugee = None;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                                WINDOW_TITLE.to_owned(),
                            ));

                            self.hex_view.purge_cache();
                            self.disassembly_view.purge_cache();
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
                                    //unstopped? unpaused? running? whatever, i'll use unstopped for consistency but it rly doesn't make sense
                                }
                            }
                        }
                    });
                });

                egui::TopBottomPanel::bottom("data")
                    .min_height(200.0)
                    .show_inside(ui, |ui| {
                        self.hex_view.show(ui, &mut self.debugee);
                    });

                egui::SidePanel::right("registers")
                    .min_width(275.0)
                    .max_width(300.0)
                    .show_inside(ui, |ui| {
                        if let Some(debugee) = self.debugee.as_mut() {
                            let is_dirty = self.regs_dirty;

                            instruction!(self, ui, debugee, rax, is_dirty);
                            instruction!(self, ui, debugee, rbx, is_dirty);
                            instruction!(self, ui, debugee, rcx, is_dirty);
                            instruction!(self, ui, debugee, rdx, is_dirty);

                            ui.separator();

                            instruction!(self, ui, debugee, r8, is_dirty);
                            instruction!(self, ui, debugee, r9, is_dirty);
                            instruction!(self, ui, debugee, r10, is_dirty);
                            instruction!(self, ui, debugee, r11, is_dirty);
                            instruction!(self, ui, debugee, r12, is_dirty);
                            instruction!(self, ui, debugee, r13, is_dirty);
                            instruction!(self, ui, debugee, r14, is_dirty);
                            instruction!(self, ui, debugee, r15, is_dirty);

                            ui.separator();

                            instruction!(self, ui, debugee, rsi, is_dirty);
                            instruction!(self, ui, debugee, rdi, is_dirty);

                            ui.separator();

                            instruction!(self, ui, debugee, rbp, is_dirty);
                            instruction!(self, ui, debugee, rsp, is_dirty);

                            ui.horizontal(|ui| {
                                ui.add_sized(egui::vec2(32.0, 4.0), egui::Label::new("EFLAGS:"));

                                ui.label(format!("{:#032b}", debugee.context().eflags));
                            });

                            self.regs_dirty = false;
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
