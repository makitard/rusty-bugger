use std::collections::HashMap;

use crate::debugger::Debugee;
use eframe::egui;

use super::widgets;

//refreshes per second
const REFRESH_RATE: f32 = 4.0;
const CACHE_RANGE: usize = 256;

pub struct HexView {
    address: u64,
    cursor_address: u64,
    cache: HashMap<u64, u8>,

    is_display_dirty: bool,
    since_last_update: std::time::SystemTime,

    render_goto_modal: bool,
    goto_input: String,
}

impl HexView {
    pub fn new() -> Self {
        Self {
            address: 0,
            cursor_address: 0,
            cache: HashMap::new(),

            is_display_dirty: false,
            since_last_update: std::time::SystemTime::UNIX_EPOCH,

            render_goto_modal: false,
            goto_input: String::new(),
        }
    }

    pub fn set_address(&mut self, address: u64) {
        self.address = address;
        self.cursor_address = address;
    }

    pub fn clean_cache(&mut self) {
        self.cache.retain(|&x, _| self.address.abs_diff(x) < CACHE_RANGE as u64 * 2);
    }

    pub fn purge_cache(&mut self) {
        self.cache.clear();
    }

    pub fn update_cache(&mut self, debugee: &mut Debugee) {
        self.since_last_update = std::time::SystemTime::now();
        let cache_start = (self.address as usize).saturating_sub(CACHE_RANGE);

        for (i, b) in debugee
            .read_memory(cache_start, CACHE_RANGE * 2)
            .into_iter()
            .enumerate()
        {
            self.cache.insert((cache_start + i) as u64, b);
        }

        self.is_display_dirty = true;
    }

    pub fn show(&mut self, ui: &mut egui::Ui, debugee: &mut Option<Debugee>) {
        if let Some(debugee) = debugee {
            if std::time::SystemTime::now()
                .duration_since(self.since_last_update)
                .unwrap_or_default()
                > std::time::Duration::from_secs_f32(1.0 / REFRESH_RATE)
            {
                self.update_cache(debugee);
                ui.ctx().request_repaint();
            }
        }

        let response = egui::Frame::default()
            .show(ui, |ui| {
                let mut i = 0;
                while ui.available_height() > 16.0 {
                    let row_address = self.address + i * 16;

                    ui.horizontal(|ui| {
                        ui.add_sized(
                            egui::vec2(100.0, 16.0),
                            egui::widgets::Label::new(
                                egui::RichText::new(format!("{row_address:#016x}")).monospace(),
                            ),
                        );

                        ui.separator();

                        let mut row_string = String::new();

                        for j in 0..16u64 {
                            let address = row_address + j;

                            let byte = self.cache.get(&address);

                            let response = if let Some(mut byte_text) =
                                byte.map(|x| format!("{x:02X}"))
                            {
                                let mut modified = false;

                                let response = ui.add_sized(
                                    egui::vec2(24.0, 24.0),
                                    widgets::editable_label(
                                        &mut byte_text,
                                        &mut modified,
                                        self.is_display_dirty,
                                        2,
                                        24.0,
                                        format!("__byte_edit_{address}"),
                                    ),
                                );

                                if address == self.cursor_address
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                {
                                    response.request_focus();
                                }

                                if response.clicked() || response.has_focus() {
                                    self.cursor_address = address;
                                }

                                if modified {
                                    if let Ok(new_value) = u8::from_str_radix(&byte_text, 16) {
                                        if let Some(debugee) = debugee {
                                            //why tf do i have to add 1 here? TODO figure this out
                                            debugee
                                                .write_memory(address as usize + 1, &[new_value]);
                                        }
                                    }
                                }

                                response
                            } else {
                                ui.add_sized(
                                    egui::vec2(24.0, 24.0),
                                    egui::Label::new(egui::RichText::new("??").monospace()),
                                )
                            };

                            if self.cursor_address == address {
                                ui.painter().rect_stroke(
                                    response.rect.expand2(egui::vec2(2.0, 1.0)),
                                    2.0,
                                    ui.style().noninteractive().bg_stroke,
                                );
                            }

                            row_string.push(
                                byte.map(|&x| {
                                    let y = x as char;
                                    if !y.is_alphanumeric() {
                                        '.'
                                    } else {
                                        y
                                    }
                                })
                                .unwrap_or('.'),
                            );
                        }

                        ui.separator();

                        ui.label(egui::RichText::new(row_string).monospace());
                    });

                    i += 1;
                }
            })
            .response;

        if ui.rect_contains_pointer(response.rect) {
            if ui
                .ctx()
                .input_mut(|x| x.consume_key(egui::Modifiers::CTRL, egui::Key::G))
            {
                self.render_goto_modal = true;
            }

            if let Some(debugee) = debugee {
                let scroll_delta = ui.input(|input| input.raw_scroll_delta);
                let estimated_bytes_per_page = response.rect.height() / 24.0;
                if scroll_delta.y < 0.0 {
                    //scroll down
                    if self
                        .cache
                        .get(
                            &self
                                .address
                                .saturating_add(estimated_bytes_per_page as u64 * 16),
                        )
                        .is_none()
                    {
                        self.update_cache(debugee);
                    }

                    self.address = self.address.saturating_add(16);

                    if self.cursor_address < self.address {
                        self.cursor_address = self.address;
                    }
                } else if scroll_delta.y > 0.0 {
                    if self
                        .cache
                        .get(
                            &self
                                .address
                                .saturating_sub(estimated_bytes_per_page as u64 * 16),
                        )
                        .is_none()
                    {
                        self.update_cache(debugee);
                    }

                    self.address = self.address.saturating_sub(16);

                    if self.cursor_address > self.address + estimated_bytes_per_page as u64 * 16 {
                        self.cursor_address = self.address + estimated_bytes_per_page as u64 * 16;
                    }
                }
            }

            ui.input_mut(|input| {
                use egui::Key as K;
                if input.consume_key(input.modifiers, K::ArrowUp) {
                    self.cursor_address = self.cursor_address.saturating_sub(16);
                }

                if input.consume_key(input.modifiers, K::ArrowDown) {
                    self.cursor_address = self.cursor_address.saturating_add(16);
                }

                if input.consume_key(input.modifiers, K::ArrowLeft) {
                    self.cursor_address = self.cursor_address.saturating_sub(1);
                }

                if input.consume_key(input.modifiers, K::ArrowRight) {
                    self.cursor_address = self.cursor_address.saturating_add(1);
                }

                let estimated_bytes_per_page = (response.rect.height() / 24.0).ceil();

                while self.cursor_address > self.address + estimated_bytes_per_page as u64 * 16 {
                    self.address = self.address.saturating_add(16);
                }

                while self.cursor_address < self.address {
                    self.address = self.address.saturating_sub(16);
                }
            });
        }

        if self.render_goto_modal {
            let mut modal = egui_modal::Modal::new(ui.ctx(), "hex_view_goto_modal")
                .with_close_on_outside_click(true);
            modal.open();

            modal.show(|ui| {
                modal.title(ui, "Go to (hex view)");

                modal.frame(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Address (hex)");
                        ui.text_edit_singleline(&mut self.goto_input);
                    });
                });

                modal.buttons(ui, |ui| {
                    if modal.suggested_button(ui, "Go").clicked() || modal.was_outside_clicked() {
                        modal.close();
                        self.render_goto_modal = false;

                        if let Some(hex_string) = self.goto_input.split('x').last() {
                            if let Ok(new_address) = u64::from_str_radix(&hex_string, 16) {
                                self.address = new_address;
                                self.cursor_address = new_address;
                            }
                        }

                        self.goto_input.clear();
                    }
                });
            });
        }

        self.is_display_dirty = false;
    }
}
