use crate::debugger::Debugee;
use eframe::egui;
use iced_x86::Formatter;

const CACHE_RANGE: u64 = 0x150;

#[derive(Clone)]
pub struct Instruction {
    addr: u64,
    bytes: Vec<u8>,
    inner: iced_x86::Instruction,
}

impl Instruction {
    //WARNING!!! THIS SUCKS
    //okay it's not that bad, man
    pub fn show(&self, ui: &mut egui::Ui, debugee: &mut Debugee, largest_instruction: usize) {
        if debugee.context().rip == self.addr {
            ui.label("▶");
        }

        let mut formatted = String::new();
        let mut formatter = iced_x86::NasmFormatter::new();
        formatter.format(&self.inner, &mut formatted);

        let mut btn_text = egui::RichText::new("○");
        if let Some(bp) = debugee.breakpoint_at_address(self.addr) {
            btn_text = egui::RichText::new("◎");
            if bp.hardware() {
                btn_text = btn_text.color(egui::Color32::LIGHT_RED);
            }
        }

        if ui
            .add(egui::Button::new(btn_text).fill(egui::Color32::from_white_alpha(0)))
            .clicked()
        {
            let mut should_remove = false;
            if let Some(bp) = debugee.breakpoint_at_address(self.addr) {
                should_remove = true;

                if !bp.hardware() {
                    debugee.add_hardware_breakpoint(self.addr);
                }
            } else {
                debugee.add_software_breakpoint(self.addr, self.bytes.len() as u64);
            }

            if should_remove {
                debugee.try_remove_breakpoint(self.addr);
            }
        }

        ui.add_sized(
            egui::vec2(100.0, 16.0),
            egui::widgets::Label::new(format!("{:#x}", self.addr)),
        );

        ui.add_sized(egui::vec2(4.0, 16.0), egui::Separator::default()); //gotta do this otherwise it takes up the entirety of the available space

        ui.add_sized(
            egui::vec2(
                10.0 * largest_instruction as f32 * 2.0, /*two chars per byte*/
                16.0,
            ),
            egui::widgets::Label::new(
                self.bytes
                    .iter()
                    .fold(String::new(), |out, byte| format!("{out} {byte:02x}")),
            ),
        );

        ui.add_sized(egui::vec2(4.0, 16.0), egui::Separator::default());

        ui.label(formatted);
    }
}

pub struct DisassemblyView {
    rip: u64,
    cache: Vec<Instruction>,

    render_goto_modal: bool,
    goto_input: String,
}

impl DisassemblyView {
    pub const fn new() -> Self {
        Self {
            rip: 0,
            cache: Vec::new(),

            render_goto_modal: false,
            goto_input: String::new(),
        }
    }

    pub fn set_rip(&mut self, rip: u64) {
        self.rip = rip;
    }

    pub fn clean_cache(&mut self) {
        self.cache
            .retain(|i| (i.addr as i128 - self.rip as i128).abs() < CACHE_RANGE as i128 * 2);
    }

    pub fn purge_cache(&mut self) {
        self.cache.clear();
    }

    pub fn refresh_cache(&mut self, debugee: &Debugee) {
        let cache_start = self.rip;
        //error handle?
        let mut data = debugee.read_memory(cache_start as usize, CACHE_RANGE as usize);
        let mut instructions = Vec::new();

        for bp in debugee.breakpoints() {
            if bp.enabled() && !bp.hardware() {
                let addr = bp.address();
                let size = bp.size() as u64;

                //is within cache bounds
                if addr >= cache_start && addr + size < cache_start + CACHE_RANGE {
                    for (i, b) in bp.original_bytes().unwrap().iter().enumerate() {
                        data[addr as usize - cache_start as usize + i] = *b;
                    }
                }
            }
        }

        let mut decoder = iced_x86::Decoder::new(64, &data, iced_x86::DecoderOptions::NONE);

        while decoder.can_decode() {
            instructions.push(decoder.decode());
        }

        self.cache.extend_from_slice(
            &instructions
                .into_iter()
                .map(|i| Instruction {
                    addr: self.rip + i.ip(),
                    bytes: data[i.ip() as usize..i.ip() as usize + i.len()].to_vec(),
                    inner: i,
                })
                .collect::<Vec<Instruction>>(),
        );

        self.cache.sort_by(|a, b| a.addr.cmp(&b.addr));
        self.cache.dedup_by(|a, b| a.addr == b.addr);

        //self.clean_cache();
    }

    pub fn show(&mut self, ui: &mut egui::Ui, debugee: &mut Debugee) {
        let rect = egui::Rect::from_min_size(ui.next_widget_position(), ui.available_size());

        let instruction_index = if let Some(index) =
            self.cache.iter().position(|x| x.addr == self.rip)
            && !self.cache.is_empty()
        {
            index
        } else {
            //rip is invalid or smth :P
            ui.label("RIP is invalid or the current memory region has not been cached");
            self.refresh_cache(debugee);
            return;
        };

        if ui.rect_contains_pointer(rect) {
            let scroll_delta = ui.input(|input| input.raw_scroll_delta);

            if scroll_delta.y < 0.0 {
                //scroll down
                let estimated_amount_of_instructions_per_page = ui.available_height() / 16.0;
                if instruction_index + estimated_amount_of_instructions_per_page as usize
                    > self.cache.len()
                {
                    self.refresh_cache(debugee);
                }

                self.rip += self.cache[instruction_index].inner.len() as u64;
            } else if scroll_delta.y > 0.0 {
                if instruction_index != 0 {
                    self.rip -= self.cache[instruction_index - 1].inner.len() as u64;
                }
            }

            if ui
                .ctx()
                .input_mut(|x| x.consume_key(egui::Modifiers::CTRL, egui::Key::G))
            {
                self.render_goto_modal = true;
            }
        }

        //unwrapping is safe here since we alr know cache is not empty
        let largest_instruction = self
            .cache
            .iter()
            .skip(instruction_index)
            .take((ui.available_height() / 16.0 * 1.4) as usize)
            .max_by(|a, b| a.inner.len().cmp(&b.inner.len()))
            .unwrap();

        let mut i = 0;
        while ui.available_height() > 16.0 {
            ui.with_layout(
                egui::Layout::left_to_right(egui::emath::Align::default()),
                |ui| {
                    self.cache[instruction_index + i].show(
                        ui,
                        debugee,
                        largest_instruction.inner.len(),
                    );
                },
            );
            ui.separator();

            i += 1;
        }

        if self.render_goto_modal {
            let mut modal = egui_modal::Modal::new(ui.ctx(), "disassembly_view_goto_modal")
                .with_close_on_outside_click(true);
            modal.open();

            modal.show(|ui| {
                modal.title(ui, "Go to (disassembly)");

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
                                self.rip = new_address;
                            }
                        }

                        self.goto_input.clear();
                    }
                });
            });
        }
    }
}
