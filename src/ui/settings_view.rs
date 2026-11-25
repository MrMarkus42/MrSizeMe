//! The settings window: everything that can be set, in one place.

use eframe::egui;
use egui::RichText;

use super::{App, Command, style};
use crate::format;
use crate::i18n::{Lang, tr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Scan,
    Ai,
    Updates,
    System,
}

/// The theme as stored in the settings (0 system, 1 light, 2 dark).
pub fn theme_preference(code: u8) -> egui::ThemePreference {
    match code {
        1 => egui::ThemePreference::Light,
        2 => egui::ThemePreference::Dark,
        _ => egui::ThemePreference::System,
    }
}

impl App {
    /// Shows the settings window if it is open; applies what changed.
    pub(super) fn settings_window(&mut self, ctx: &egui::Context) -> Vec<Command> {
        let Some(mut tab) = self.settings_tab else { return Vec::new() };
        let t = tr();
        let before = self.settings.clone();
        let mut cmds = Vec::new();
        let mut error = None;
        let mut open = true;
        egui::Window::new(t.btn_settings)
            .id(egui::Id::unique("settings_window"))
            .open(&mut open)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .default_width(600.0)
            .default_height(520.0)
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (value, label) in [
                        (SettingsTab::General, t.set_tab_general),
                        (SettingsTab::Scan, t.set_tab_scan),
                        (SettingsTab::Ai, t.set_tab_ai),
                        (SettingsTab::Updates, t.set_tab_updates),
                        (SettingsTab::System, t.set_tab_system),
                    ] {
                        ui.selectable_value(&mut tab, value, RichText::new(label).strong());
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().id_salt("settings_scroll").show(ui, |ui| match tab {
                    SettingsTab::General => self.general(ui),
                    SettingsTab::Scan => self.scanning(ui),
                    SettingsTab::Ai => {
                        error = super::cleanup::ai_section(ui, &mut self.cleanup, &mut self.settings);
                    }
                    SettingsTab::Updates => self.update_settings(ui, &mut cmds),
                    SettingsTab::System => self.system(ui, &mut cmds),
                });
            });
        self.settings_tab = open.then_some(tab);
        if let Some(e) = error {
            self.show_error(e);
        }
        self.settings_changed(ctx, &before);
        cmds
    }

    /// Side effects of changed settings, then saving.
    fn settings_changed(&mut self, ctx: &egui::Context, before: &crate::settings::Settings) {
        let s = self.settings.clone();
        if s == *before {
            return;
        }
        if s.lang != before.lang {
            crate::i18n::set_lang(s.lang.unwrap_or_else(Lang::from_system));
            self.ribbon_widths.clear();
        }
        if s.zoom != before.zoom {
            ctx.set_zoom_factor(f32::from(s.zoom) / 100.0);
            self.ribbon_widths.clear();
        }
        if s.theme != before.theme {
            ctx.set_theme(theme_preference(s.theme));
        }
        if !s.live {
            self.live = None;
        }
        if s.live != before.live {
            self.live_too_large = false;
        }
        if s.warn_percent != before.warn_percent {
            self.warn.low.clear();
            if s.warn_percent > 0 {
                self.check_space(false);
            }
        }
        self.save_settings();
    }

    fn general(&mut self, ui: &mut egui::Ui) {
        let t = tr();
        let s = &mut self.settings;
        egui::Grid::new("general_grid").num_columns(2).spacing([16.0, 10.0]).show(ui, |ui| {
            ui.label(t.set_language);
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut s.lang, None, t.set_lang_system);
                for l in Lang::ALL {
                    ui.selectable_value(&mut s.lang, Some(l), l.native_name());
                }
            });
            ui.end_row();
            ui.label(t.set_theme);
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut s.theme, 0, t.theme_system);
                ui.selectable_value(&mut s.theme, 1, t.theme_light);
                ui.selectable_value(&mut s.theme, 2, t.theme_dark);
            });
            ui.end_row();
            ui.label(t.set_text_size);
            ui.vertical(|ui| {
                ui.horizontal_wrapped(|ui| {
                    for z in style::ZOOMS {
                        ui.selectable_value(&mut s.zoom, z, format!("{z} %"));
                    }
                });
                let hint = if cfg!(target_os = "macos") {
                    t.set_text_size_hint.replace("Strg", "⌘").replace("Ctrl", "⌘")
                } else {
                    t.set_text_size_hint.to_string()
                };
                ui.label(RichText::new(hint).small().weak());
            });
            ui.end_row();
        });
    }

    fn scanning(&mut self, ui: &mut egui::Ui) {
        let t = tr();
        let s = &mut self.settings;
        ui.checkbox(&mut s.history, t.btn_keep_history);
        ui.label(RichText::new(t.tip_keep_history).small().weak());
        ui.add_space(8.0);
        ui.checkbox(&mut s.live, t.btn_live);
        ui.label(RichText::new(t.tip_live).small().weak());
        ui.add_space(8.0);
        ui.strong(t.btn_warning);
        ui.horizontal_wrapped(|ui| {
            for p in [0u8, 5, 10, 15, 20] {
                let label =
                    if p == 0 { t.warn_off.to_string() } else { t.warn_below.replace("{n}", &p.to_string()) };
                ui.selectable_value(&mut s.warn_percent, p, label);
            }
        });
    }

    fn update_settings(&mut self, ui: &mut egui::Ui, cmds: &mut Vec<Command>) {
        let t = tr();
        ui.label(t.set_version.replace("{v}", crate::update::current_version()));
        let last = self.settings.update_last;
        ui.label(
            RichText::new(if last > 0 {
                t.set_last_check.replace("{date}", &format::date_time(last))
            } else {
                t.set_never_checked.to_string()
            })
            .weak(),
        );
        ui.add_space(8.0);
        ui.strong(t.updates_auto);
        let auto = self.settings.updates == Some(true);
        let every = self.settings.update_every_days;
        ui.horizontal_wrapped(|ui| {
            for (value, label) in [
                (None, t.updates_off),
                (Some(1u16), t.updates_daily),
                (Some(7), t.updates_weekly),
                (Some(30), t.updates_monthly),
            ] {
                let on = match value {
                    None => !auto,
                    Some(d) => auto && every == d,
                };
                if ui.selectable_label(on, label).clicked() {
                    self.settings.updates = Some(value.is_some());
                    if let Some(d) = value {
                        self.settings.update_every_days = d;
                    }
                }
            }
        });
        ui.label(RichText::new(t.updates_ask_text.split("\n\n").nth(1).unwrap_or_default()).small().weak());
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.add_enabled(!self.upd.busy(), egui::Button::new(t.btn_check_updates)).clicked() {
                cmds.push(Command::CheckUpdates);
            }
            if self.upd.busy() {
                ui.spinner();
                ui.label(t.updates_checking);
            }
        });
        if let Some(r) = &self.upd.found {
            ui.add_space(6.0);
            ui.colored_label(ui.visuals().warn_fg_color, t.set_update_ready.replace("{v}", &r.version));
        }
    }

    fn system(&mut self, ui: &mut egui::Ui, cmds: &mut Vec<Command>) {
        let t = tr();
        ui.checkbox(&mut self.settings.offline, t.btn_offline);
        ui.label(RichText::new(t.tip_offline).small().weak());
        ui.add_space(10.0);
        ui.strong(t.btn_context_menu);
        ui.label(RichText::new(t.tip_context_menu).small().weak());
        let on = *self.shell_installed.get_or_insert_with(crate::shell::is_installed);
        if ui.button(if on { t.btn_remove } else { t.btn_add }).clicked() {
            cmds.push(Command::AskShell(!on));
        }
        ui.add_space(10.0);
        ui.strong(t.grp_history);
        if ui.add_enabled(self.history_store.is_some(), egui::Button::new(t.btn_clear_history)).clicked() {
            cmds.push(Command::AskClearHistory);
        }
        ui.add_space(10.0);
        if ui.button(t.log_folder).clicked() {
            cmds.push(Command::LogFolder);
        }
    }
}
