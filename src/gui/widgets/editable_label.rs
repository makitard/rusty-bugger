use std::hash::Hash;

use eframe::egui::{self, Response, Ui};

#[derive(Default, Clone)]
struct EditableLabelMemory {
    focused: bool,
    intermediate_buffer: String,
    update_scheduled: bool,
}

impl EditableLabelMemory {
    pub fn load(ctx: &egui::Context, id: egui::Id) -> Option<Self> {
        ctx.data_mut(|d| d.get_temp(id))
    }

    pub fn store(self, ctx: &egui::Context, id: egui::Id) {
        ctx.data_mut(|d| d.insert_temp(id, self));
    }
}

fn editable_label_ui<'a>(
    ui: &mut Ui,
    buffer: &'a mut String,
    modified: &'a mut bool,
    force_update: bool,
    max_chars: usize,
    width: f32,
    id: egui::Id,
) -> Response {
    let mut memory = EditableLabelMemory::load(ui.ctx(), id).unwrap_or_default();

    if force_update {
        memory.update_scheduled = true;
    }

    if memory.focused {
        let response = ui.add(
            egui::TextEdit::singleline(&mut memory.intermediate_buffer)
                .code_editor()
                .char_limit(max_chars)
                .desired_width(width),
        );

        if response.clicked_elsewhere() || response.lost_focus() {
            memory.focused = false;

            *buffer = memory.intermediate_buffer.clone();
            *modified = true;
            memory.update_scheduled = false;
        }

        memory.store(ui.ctx(), id);

        response
    } else {
        let response = ui.add(
            egui::Button::new(egui::RichText::new(buffer.clone()).monospace())
                .fill(egui::Color32::TRANSPARENT),
        );

        if memory.update_scheduled {
            memory.intermediate_buffer = buffer.clone();
            memory.update_scheduled = false;
        }

        if memory.intermediate_buffer.is_empty() {
            memory.intermediate_buffer = buffer.clone();
        }

        if response.clicked() || response.gained_focus() {
            memory.focused = true;
        }

        memory.store(ui.ctx(), id);

        response
    }
}

pub fn editable_label<'a>(
    buffer: &'a mut String,
    modified: &'a mut bool,
    force_update: bool,
    max_chars: usize,
    width: f32,
    id: impl Hash + 'a,
) -> impl egui::Widget + 'a {
    move |ui: &mut egui::Ui| {
        editable_label_ui(
            ui,
            buffer,
            modified,
            force_update,
            max_chars,
            width,
            egui::Id::new(id),
        )
    }
}
