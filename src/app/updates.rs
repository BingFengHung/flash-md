use super::*;

impl MdPreviewApp {
    pub(super) fn handle_update_event(&mut self, event: UpdateEvent) {
        match event {
            UpdateEvent::Checked(Ok(Some(release))) => {
                self.set_toast(format!("🎉 發現新版本 {}", release.tag_name));
                self.available_update = Some(release);
            }
            UpdateEvent::Checked(Ok(None)) => {
                self.set_toast(format!("✅ 目前已是最新版本 (v{})", CURRENT_VERSION))
            }
            UpdateEvent::Checked(Err(error)) => {
                self.set_toast(format!("❌ 無法檢查更新：{}", error))
            }
            UpdateEvent::Installed(result) => {
                self.is_updating = false;
                match result {
                    Ok(()) => {
                        let args = if self.is_standalone {
                            self.current_file
                                .as_ref()
                                .map(|p| vec![p.to_string_lossy().into_owned()])
                                .unwrap_or_default()
                        } else {
                            Vec::new()
                        };
                        match restart_with_new_version(&args) {
                            Ok(()) => std::process::exit(0),
                            Err(error) => {
                                self.set_toast(format!("❌ {}；請手動重新啟動程式", error))
                            }
                        }
                    }
                    Err(error) => self.set_toast(format!("❌ 更新失敗：{}", error)),
                }
            }
        }
    }

    pub fn check_update_manually(&mut self) {
        self.set_toast("正在檢查 GitHub 最新版本... ⏳".to_string());
        let tx = self.update_tx.clone();
        let ctx_holder = self.ctx_holder.clone();
        thread::spawn(move || {
            let rel = check_latest_release();
            let _ = tx.send(UpdateEvent::Checked(rel));
            if let Ok(guard) = ctx_holder.lock() {
                if let Some(ref ctx) = *guard {
                    ctx.request_repaint();
                }
            }
        });
    }

    pub fn trigger_self_update(&mut self) {
        self.request_action(PendingAction::Update);
    }

    pub(super) fn start_self_update(&mut self) {
        if self.is_updating {
            return;
        }
        let Some(release) = self.available_update.clone() else {
            return;
        };
        self.is_updating = true;
        self.set_toast(format!("正在更新至 {}…", release.tag_name));
        let tx = self.update_tx.clone();
        let holder = self.ctx_holder.clone();
        thread::spawn(move || {
            let result = perform_self_update(&release);
            let _ = tx.send(UpdateEvent::Installed(result));
            if let Ok(holder) = holder.lock() {
                if let Some(ctx) = holder.as_ref() {
                    ctx.request_repaint();
                }
            }
        });
    }
}
