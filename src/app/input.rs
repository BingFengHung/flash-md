use super::*;

impl MdPreviewApp {
    pub(super) fn sync_preferences(&mut self) {
        if self.config.theme != self.theme
            || self.config.font_scale != self.font_scale
            || self.config.always_on_top != self.always_on_top
        {
            self.config.theme = self.theme;
            self.config.font_scale = self.font_scale;
            self.config.always_on_top = self.always_on_top;
            self.config.save();
        }
    }

    pub(super) fn process_timers(&mut self, ctx: &Context) {
        if self.config.save_mode == SaveMode::AutoDebounce && self.is_modified {
            if let Some(instant) = self.last_edit_instant {
                let delay = Duration::from_millis(800);
                let elapsed = instant.elapsed();
                if elapsed >= delay {
                    self.save_current_file(true);
                    self.last_edit_instant = None;
                } else {
                    ctx.request_repaint_after(delay - elapsed);
                }
            }
        }
        if let Some((_, instant)) = &self.status_toast {
            let lifetime = Duration::from_secs(4);
            let elapsed = instant.elapsed();
            if elapsed >= lifetime {
                self.status_toast = None;
            } else {
                ctx.request_repaint_after(lifetime - elapsed);
            }
        }
    }

    pub(super) fn handle_ime_input(&mut self, ctx: &Context) {
        if !self.is_editing {
            self.is_ime_composing = false;
            self.last_ime_activity = None;
            return;
        }
        let recent = self
            .last_ime_activity
            .is_some_and(|instant| instant.elapsed() < Duration::from_millis(400));
        let mut active = self.is_ime_composing;
        let mut swallowed = false;
        ctx.input_mut(|input| {
            for event in &input.events {
                match event {
                    egui::Event::Ime(egui::ImeEvent::Preedit(text)) => {
                        self.is_ime_composing = !text.is_empty();
                        active |= self.is_ime_composing;
                    }
                    egui::Event::Ime(egui::ImeEvent::Commit(_)) => {
                        active = true;
                        self.is_ime_composing = false;
                    }
                    egui::Event::Ime(egui::ImeEvent::Disabled) => self.is_ime_composing = false,
                    _ => {}
                }
            }
            if active {
                self.last_ime_activity = Some(std::time::Instant::now());
            }
            if active || recent {
                input.events.retain(|event| {
                    let enter = matches!(event, egui::Event::Key { key: egui::Key::Enter, .. })
                        || matches!(event, egui::Event::Text(text) if matches!(text.as_str(), "\n" | "\r" | "\r\n"));
                    swallowed |= enter;
                    !enter
                });
                if swallowed {
                    input.keys_down.remove(&egui::Key::Enter);
                }
            }
        });
        if swallowed
            || ctx
                .input(|input| input.pointer.any_click() || input.key_pressed(egui::Key::Backspace))
        {
            self.last_ime_activity = None;
            self.is_ime_composing = false;
        }
    }
}
