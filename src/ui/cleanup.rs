//! Clean-up help: suggestion list, the "What is this?" window with the
//! AI question, the recycle bin question and the AI settings.

use std::io;
use std::thread::JoinHandle;

use eframe::egui;
use egui::{Color32, RichText};
use egui_extras::{Column, TableBuilder};

use super::Action;
use crate::ai::{self, Provider};
use crate::format;
use crate::i18n::tr;
use crate::knowledge::{self, Light, Suggestion};
use crate::secrets;
use crate::settings::Settings;
use crate::tree::{NodeId, Tree};
use crate::{applog, trash};

/// Folders smaller than this are not suggested.
const MIN_SUGGESTION: u64 = 50 * 1024 * 1024;

pub fn light_color(l: Light) -> Color32 {
    match l {
        Light::Green => Color32::from_rgb(34, 197, 94),
        Light::Yellow => Color32::from_rgb(234, 179, 8),
        Light::Red => Color32::from_rgb(239, 68, 68),
    }
}

fn light_badge(ui: &mut egui::Ui, l: Light) {
    let t = tr();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 5.5, light_color(l));
    ui.label(RichText::new(t.lights[l as usize]).strong().color(light_color(l)));
}

pub struct State {
    /// The node shown in the "What is this?" window.
    pub info: Option<NodeId>,
    /// The node waiting for the recycle bin question.
    pub confirm: Option<NodeId>,
    pub provider: Provider,
    running: Option<JoinHandle<io::Result<String>>>,
    answer: Option<Result<String, String>>,
    /// The user asked for the AI settings (the settings window opens there).
    pub show_settings: bool,
    /// Keys being typed, per service.
    key_inputs: std::collections::HashMap<&'static str, String>,
    /// Which services have a stored key (looked up once).
    stored: std::collections::HashMap<&'static str, bool>,
    ollama_models: Vec<String>,
    router_job: Option<JoinHandle<io::Result<Vec<ai::RouterModel>>>>,
    router_models: Vec<ai::RouterModel>,
    router_filter: String,
    router_free_only: bool,
    router_error: Option<String>,
    suggestions: Option<(u64, NodeId, Vec<Suggestion>)>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            info: None,
            confirm: None,
            provider: Provider::GoogleAi,
            running: None,
            answer: None,
            show_settings: false,
            key_inputs: std::collections::HashMap::new(),
            stored: std::collections::HashMap::new(),
            ollama_models: Vec::new(),
            router_job: None,
            router_models: Vec::new(),
            router_filter: String::new(),
            router_free_only: false,
            router_error: None,
            suggestions: None,
        }
    }
}

impl State {
    /// Opens the "What is this?" window for a node.
    pub fn open_info(&mut self, id: NodeId) {
        if self.info != Some(id) {
            self.answer = None;
        }
        self.info = Some(id);
    }

    /// Forgets everything that refers to nodes (after a new scan).
    pub fn reset(&mut self) {
        self.info = None;
        self.confirm = None;
        self.answer = None;
        self.suggestions = None;
    }

    /// True if a key is stored for the service (asked once, then remembered).
    fn key_stored(&mut self, service: &'static str) -> bool {
        *self.stored.entry(service).or_insert_with(|| secrets::get(service).is_some())
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some(h) = &self.router_job {
            if h.is_finished() {
                match self.router_job.take().map(|h| h.join()) {
                    Some(Ok(Ok(list))) => {
                        self.router_models = list;
                        self.router_error = None;
                    }
                    Some(Ok(Err(e))) => self.router_error = Some(e.to_string()),
                    _ => {}
                }
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(150));
            }
        }
        let Some(h) = &self.running else { return };
        if !h.is_finished() {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
            return;
        }
        if let Some(h) = self.running.take() {
            self.answer = Some(match h.join() {
                Ok(Ok(text)) => Ok(text),
                Ok(Err(e)) => Err(e.to_string()),
                Err(_) => Err("internal error".into()),
            });
        }
    }
}

/// The list of clean-up suggestions below `start`.
pub fn list(
    ui: &mut egui::Ui,
    tree: &Tree,
    start: NodeId,
    generation: u64,
    state: &mut State,
) -> Vec<Action> {
    let t = tr();
    if state.suggestions.as_ref().is_none_or(|s| s.0 != generation || s.1 != start) {
        state.suggestions = Some((generation, start, knowledge::suggestions(tree, start, MIN_SUGGESTION)));
    }
    let Some((_, _, items)) = &state.suggestions else { return Vec::new() };
    let mut actions = Vec::new();
    ui.label(RichText::new(tree.path(start).display().to_string()).weak());
    if items.is_empty() {
        ui.add_space(8.0);
        ui.label(t.cleanup_none);
        return actions;
    }
    let sum = |l: Light| items.iter().filter(|s| s.light == l).map(|s| s.size).sum::<u64>();
    let mut summary = t.cleanup_summary.replace("{green}", &format::size(sum(Light::Green)));
    if sum(Light::Yellow) > 0 {
        summary.push(' ');
        summary.push_str(&t.cleanup_summary_tools.replace("{yellow}", &format::size(sum(Light::Yellow))));
    }
    ui.add_space(4.0);
    ui.label(RichText::new(summary).strong());
    ui.add_space(4.0);
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .sense(egui::Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::initial(24.0))
        .column(Column::initial(230.0).at_least(120.0).clip(true))
        .column(Column::initial(84.0))
        .column(Column::remainder().at_least(100.0).clip(true))
        .header(22.0, |mut h| {
            for label in ["", t.col_name, t.col_size, t.col_path] {
                h.col(|ui| {
                    ui.strong(label);
                });
            }
        })
        .body(|body| {
            body.rows(24.0, items.len(), |mut row| {
                let s = &items[row.index()];
                let entry = knowledge::by_id(s.entry);
                row.col(|ui| {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                    ui.painter().circle_filled(rect.center(), 6.0, light_color(s.light));
                });
                row.col(|ui| {
                    ui.add(egui::Label::new(entry.map_or("", |e| e.text().0)).truncate().selectable(false));
                });
                row.col(|ui| {
                    ui.label(format::size(s.size));
                });
                row.col(|ui| {
                    let p = applog::private(&tree.path(s.id));
                    ui.add(egui::Label::new(RichText::new(p).weak()).truncate().selectable(false));
                });
                let resp = row.response();
                if resp.clicked() {
                    actions.push(Action::Select(s.id));
                }
                if resp.double_clicked() {
                    actions.push(Action::Info(s.id));
                }
                resp.context_menu(|ui| super::folders::context_menu(ui, s.id, &mut actions));
            });
        });
    actions
}

/// The "What is this? May it go?" window with the AI question.
pub fn info_window(ctx: &egui::Context, tree: &Tree, state: &mut State, settings: &Settings) -> Vec<Action> {
    let mut actions = Vec::new();
    let Some(id) = state.info else { return actions };
    let t = tr();
    let n = tree.node(id);
    let path = tree.path(id);
    let entry = knowledge::lookup(tree, id);
    let protection = trash::protection(&path);
    let mut open = true;
    let has_key: std::collections::HashMap<&str, bool> =
        Provider::ALL.iter().filter_map(|p| p.key_service()).map(|s| (s, state.key_stored(s))).collect();
    egui::Window::new(t.info_title)
        .id(egui::Id::unique("info_window"))
        .open(&mut open)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .default_width(560.0)
        .resizable(true)
        .collapsible(false)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(ctx.content_rect().height() * 0.75).show(ui, |ui| {
                let name = if id == crate::tree::ROOT {
                    path.display().to_string()
                } else {
                    n.name_lossy().into_owned()
                };
                ui.heading(name);
                ui.label(
                    RichText::new(format!("{}  ·  {}", applog::private(&path), format::size(n.size))).weak(),
                );
                ui.add_space(6.0);
                match entry {
                    Some(e) => {
                        let (title, what, how) = e.text();
                        ui.horizontal(|ui| light_badge(ui, e.light));
                        ui.strong(title);
                        ui.add_space(4.0);
                        ui.label(RichText::new(t.info_what).underline());
                        ui.add(egui::Label::new(what).selectable(true));
                        ui.add_space(4.0);
                        ui.label(RichText::new(t.info_how).underline());
                        ui.add(egui::Label::new(how).selectable(true));
                    }
                    None => {
                        ui.label(t.info_unknown);
                    }
                }
                ui.add_space(6.0);
                let red = entry.is_some_and(|e| e.light == Light::Red);
                if protection.is_some() || red {
                    ui.colored_label(light_color(Light::Red), t.protected);
                }
                ui.horizontal(|ui| {
                    if ui.add_enabled(protection.is_none() && !red, egui::Button::new(t.btn_trash)).clicked()
                    {
                        actions.push(Action::AskTrash(id));
                    }
                    if ui.button(t.open_folder).clicked() {
                        actions.push(Action::OpenFolder(id));
                    }
                });

                ui.separator();
                ui.strong(t.ai_heading);
                ui.horizontal_wrapped(|ui| {
                    for p in Provider::ALL {
                        if ui.selectable_value(&mut state.provider, p, t.providers[p.index()]).changed() {
                            state.answer = None;
                        }
                    }
                });
                let question = ai::question(tree, id);
                ui.label(RichText::new(t.ai_sent_text).small().weak());
                let mut shown = question.as_str();
                ui.add(egui::TextEdit::multiline(&mut shown).desired_rows(2).desired_width(f32::INFINITY));
                let p = state.provider;
                let blocked_offline = settings.offline && p.is_online();
                let missing = match p {
                    _ if p.key_service().is_some_and(|s| !has_key.get(s).copied().unwrap_or(false)) => {
                        Some(t.ai_no_key)
                    }
                    Provider::Ollama if settings.ollama_model.trim().is_empty() => Some(t.ai_no_model),
                    Provider::OpenRouter if settings.openrouter_model.trim().is_empty() => {
                        Some(t.ai_no_model)
                    }
                    _ => None,
                };
                if blocked_offline {
                    ui.colored_label(ui.visuals().warn_fg_color, t.ai_offline_note);
                } else if let Some(m) = missing {
                    ui.horizontal(|ui| {
                        ui.colored_label(ui.visuals().warn_fg_color, m);
                        if ui.button(t.btn_ai_settings).clicked() {
                            state.show_settings = true;
                        }
                    });
                }
                ui.horizontal(|ui| {
                    let busy = state.running.is_some();
                    if p.uses_browser() {
                        if ui.add_enabled(!blocked_offline, egui::Button::new(t.ai_open_browser)).clicked() {
                            let url = ai::google_url(&question, p == Provider::GoogleAi);
                            if let Err(e) = crate::external::open_url(&url) {
                                state.answer = Some(Err(e.to_string()));
                            }
                        }
                    } else if ui
                        .add_enabled(
                            !blocked_offline && missing.is_none() && !busy,
                            egui::Button::new(t.ai_send),
                        )
                        .clicked()
                    {
                        let key = p.key_service().and_then(secrets::get);
                        let settings = settings.clone();
                        let q = question.clone();
                        log::info!("AI question sent ({p:?})");
                        state.answer = None;
                        state.running = std::thread::Builder::new()
                            .name("ai".into())
                            .spawn(move || ai::ask(p, &q, &settings, key.as_deref()))
                            .ok();
                    }
                    if busy {
                        ui.spinner();
                        ui.label(t.ai_waiting);
                    }
                });
                if let Some(answer) = &state.answer {
                    ui.add_space(4.0);
                    ui.strong(t.ai_answer);
                    match answer {
                        Ok(text) => {
                            ui.add(egui::Label::new(text.as_str()).selectable(true));
                        }
                        Err(e) => {
                            ui.colored_label(ui.visuals().error_fg_color, format!("{}: {e}", t.error_title));
                        }
                    }
                }
                ui.add_space(4.0);
                ui.label(RichText::new(t.ai_warning).small().italics());
            });
        });
    if !open {
        state.info = None;
    }
    actions
}

/// The recycle bin question. Returns the node to move when confirmed.
pub fn confirm_window(ctx: &egui::Context, tree: &Tree, state: &mut State) -> Option<NodeId> {
    let id = state.confirm?;
    let t = tr();
    let n = tree.node(id);
    let path = tree.path(id);
    let entry = knowledge::lookup(tree, id);
    let allowed = trash::protection(&path).is_none() && !entry.is_some_and(|e| e.light == Light::Red);
    let mut result = None;
    let mut close = false;
    egui::Window::new(t.trash_title)
        .id(egui::Id::unique("trash_window"))
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .collapsible(false)
        .resizable(false)
        .default_width(420.0)
        .show(ctx, |ui| {
            let name = n.name_lossy();
            ui.label(t.trash_text.replace("{name}", &name).replace("{size}", &format::size(n.size)));
            ui.label(RichText::new(applog::private(&path)).weak().small());
            if entry.is_some_and(|e| e.light == Light::Yellow && !e.can_trash) {
                ui.colored_label(light_color(Light::Yellow), t.trash_tool_warning);
            }
            if !allowed {
                ui.colored_label(light_color(Light::Red), t.protected);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(allowed, egui::Button::new(RichText::new(t.btn_trash).strong())).clicked() {
                    result = Some(id);
                    close = true;
                }
                if ui.button(t.cancel).clicked() {
                    close = true;
                }
            });
        });
    if close {
        state.confirm = None;
    }
    result
}

/// The AI part of the settings window. Returns an error text, if any.
pub fn ai_section(ui: &mut egui::Ui, state: &mut State, settings: &mut Settings) -> Option<String> {
    let t = tr();
    let mut error = None;

    // Preselected AI in "What is this?".
    ui.strong(t.set_default_ai);
    ui.horizontal_wrapped(|ui| {
        for p in Provider::ALL {
            if ui.selectable_value(&mut settings.ai_provider, p, t.providers[p.index()]).changed() {
                state.provider = p;
            }
        }
    });
    ui.separator();

    // Local AI.
    ui.strong(t.providers[Provider::Ollama.index()]);
    ui.label(RichText::new(t.ai_ollama_hint).small().weak());
    egui::Grid::new("ollama_grid").num_columns(2).show(ui, |ui| {
        let l = ui.label(t.ai_address);
        ui.add(egui::TextEdit::singleline(&mut settings.ollama_url).desired_width(260.0)).labelled_by(l.id);
        ui.end_row();
        let l = ui.label(t.ai_model);
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut settings.ollama_model).desired_width(180.0))
                .labelled_by(l.id);
            if ui.button(t.ai_load_models).clicked() {
                match ai::ollama_models(settings) {
                    Ok(m) => state.ollama_models = m,
                    Err(e) => error = Some(format!("Ollama: {e}")),
                }
            }
        });
        ui.end_row();
    });
    if !state.ollama_models.is_empty() {
        ui.horizontal_wrapped(|ui| {
            for m in &state.ollama_models {
                if ui.selectable_label(settings.ollama_model == *m, m).clicked() {
                    settings.ollama_model = m.clone();
                }
            }
        });
    }
    ui.separator();

    // Services with the user's own key.
    for p in [Provider::OpenRouter, Provider::Gemini, Provider::Claude] {
        let service = p.key_service().expect("these need a key");
        // For the model list (only the offline switch matters there).
        let for_list = settings.clone();
        let stored = state.key_stored(service);
        ui.push_id(service, |ui| {
            ui.strong(t.providers[p.index()]);
            if p == Provider::OpenRouter {
                ui.label(RichText::new(t.ai_openrouter_hint).small().weak());
            }
            ui.label(RichText::new(if stored { t.ai_key_stored } else { t.ai_key_missing }).small());
            egui::Grid::new("key_grid").num_columns(2).show(ui, |ui| {
                let l = ui.label(t.ai_key);
                ui.horizontal(|ui| {
                    let input = state.key_inputs.entry(service).or_default();
                    ui.add(egui::TextEdit::singleline(input).password(true).desired_width(220.0))
                        .labelled_by(l.id);
                    if ui.add_enabled(!input.trim().is_empty(), egui::Button::new(t.ai_save)).clicked() {
                        match secrets::set(service, input.trim()) {
                            Ok(()) => {
                                input.clear();
                                state.stored.remove(service);
                            }
                            Err(e) => error = Some(e.to_string()),
                        }
                    }
                    if ui.add_enabled(stored, egui::Button::new(t.ai_remove)).clicked() {
                        if let Err(e) = secrets::delete(service) {
                            error = Some(e.to_string());
                        }
                        state.stored.remove(service);
                    }
                });
                ui.end_row();
                let model = match p {
                    Provider::OpenRouter => &mut settings.openrouter_model,
                    Provider::Gemini => &mut settings.gemini_model,
                    _ => &mut settings.claude_model,
                };
                let l = ui.label(t.ai_model);
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(model).desired_width(220.0)).labelled_by(l.id);
                    if p == Provider::OpenRouter {
                        let busy = state.router_job.is_some();
                        if ui.add_enabled(!busy, egui::Button::new(t.ai_load_models)).clicked() {
                            let s = for_list.clone();
                            state.router_job = std::thread::Builder::new()
                                .name("openrouter models".into())
                                .spawn(move || ai::openrouter_models(&s))
                                .ok();
                        }
                        if busy {
                            ui.spinner();
                        }
                    }
                });
                ui.end_row();
            });
            if p == Provider::OpenRouter {
                router_models(ui, state, settings);
            }
        });
        ui.add_space(6.0);
    }
    ui.label(RichText::new(t.ai_key_hint).small().weak());
    error
}

/// The model list of OpenRouter with search and "free only".
fn router_models(ui: &mut egui::Ui, state: &mut State, settings: &mut Settings) {
    let t = tr();
    if let Some(e) = &state.router_error {
        ui.colored_label(ui.visuals().error_fg_color, format!("OpenRouter: {e}"));
    }
    if state.router_models.is_empty() {
        return;
    }
    ui.horizontal(|ui| {
        let l = ui.label(t.ai_search);
        ui.add(egui::TextEdit::singleline(&mut state.router_filter).desired_width(180.0)).labelled_by(l.id);
        ui.checkbox(&mut state.router_free_only, t.ai_only_free);
    });
    let needle = state.router_filter.to_lowercase();
    let shown: Vec<&ai::RouterModel> = state
        .router_models
        .iter()
        .filter(|m| !state.router_free_only || m.free)
        .filter(|m| {
            needle.is_empty()
                || m.id.to_lowercase().contains(&needle)
                || m.name.to_lowercase().contains(&needle)
        })
        .take(300)
        .collect();
    egui::ScrollArea::vertical().id_salt("router_list").max_height(180.0).show(ui, |ui| {
        for m in shown {
            let mut text = format!("{}  ({})", m.name, m.id);
            if m.free {
                text = format!("{text}  · {}", t.ai_free);
            }
            if ui.selectable_label(settings.openrouter_model == m.id, text).clicked() {
                settings.openrouter_model = m.id.clone();
            }
        }
    });
}
