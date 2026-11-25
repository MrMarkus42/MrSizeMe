//! macOS: the menu bar at the top of the screen. The menu band inside the
//! window stays; this menu offers the same commands the Mac way.

use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

use super::{Chart, Command, View, settings_view::SettingsTab};
use crate::i18n::tr;

pub struct MacMenu {
    _menu: Menu,
    events: Receiver<MenuEvent>,
}

fn item(id: &str, text: &str, key: Option<(Modifiers, Code)>) -> MenuItem {
    MenuItem::with_id(id, text, true, key.map(|(m, c)| Accelerator::new(m, c)))
}

const CMD: Modifiers = Modifiers::META;

/// Builds the menu bar in the current language and hands it to the app.
pub fn build(ctx: &egui::Context) -> Option<MacMenu> {
    let t = tr();
    let (tx, events) = channel();
    let c = ctx.clone();
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        let _ = tx.send(e);
        c.request_repaint();
    }));

    let menu = Menu::new();
    let app = Submenu::new("MrSizeMe", true);
    let file = Submenu::new(t.mac_file, true);
    let edit = Submenu::new(t.mac_edit, true);
    let view = Submenu::new(t.tab_view, true);
    let window = Submenu::new(t.mac_window, true);
    let help = Submenu::new(t.tab_help, true);

    let result: muda::Result<()> = (|| {
        app.append_items(&[
            &item("about", &format!("{} MrSizeMe", t.mac_about), None),
            &PredefinedMenuItem::separator(),
            &item("settings", &format!("{} …", t.btn_settings), Some((CMD, Code::Comma))),
            &item("updates", &format!("{} …", t.btn_check_updates), None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(None),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::quit(None),
        ])?;
        file.append_items(&[
            &item("open", &format!("{} …", t.btn_folder), Some((CMD, Code::KeyO))),
            &item("rescan", t.rescan, Some((CMD, Code::KeyR))),
            &item("fda", &format!("{} …", t.btn_full_access), None),
            &PredefinedMenuItem::separator(),
            &item("csv", t.btn_csv, Some((CMD, Code::KeyE))),
            &item("xlsx", t.btn_xlsx, None),
            &item("html", t.btn_html, None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::close_window(None),
        ])?;
        // No standard copy/paste items: they would take ⌘C/⌘V away from the text fields.
        edit.append_items(&[
            &item("copy_path", t.copy_path, None),
            &item("search", t.view_search, Some((CMD, Code::KeyF))),
        ])?;
        view.append_items(&[
            &item("v_folders", t.view_folders, Some((CMD, Code::Digit1))),
            &item("v_types", t.view_types, Some((CMD, Code::Digit2))),
            &item("v_categories", t.view_categories, Some((CMD, Code::Digit3))),
            &item("v_largest", t.view_largest, Some((CMD, Code::Digit4))),
            &item("v_oldest", t.view_oldest, None),
            &item("v_age", t.view_age, None),
            &item("v_cleanup", t.view_cleanup, None),
            &item("v_history", t.view_history, None),
            &item("v_duplicates", t.view_duplicates, None),
            &item("v_cloud", t.view_cloud, None),
            &item("v_apps", t.view_apps, None),
            &PredefinedMenuItem::separator(),
            &item("c_map", t.map_title, None),
            &item("c_ring", t.ring_title, None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::fullscreen(None),
        ])?;
        window.append_items(&[
            &PredefinedMenuItem::minimize(None),
            &PredefinedMenuItem::maximize(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::bring_all_to_front(None),
        ])?;
        help.append_items(&[
            &item("shortcuts", t.shortcuts, None),
            &item("log", t.log_folder, None),
            &item("website", t.mac_website, None),
        ])?;
        menu.append_items(&[&app, &file, &edit, &view, &window, &help])?;
        window.set_as_windows_menu_for_nsapp();
        help.set_as_help_menu_for_nsapp();
        menu.init_for_nsapp();
        Ok(())
    })();
    if let Err(e) = result {
        log::warn!("menu bar not built: {e}");
        return None;
    }
    Some(MacMenu { _menu: menu, events })
}

impl MacMenu {
    /// The commands the user chose in the menu since the last frame.
    pub fn commands(&self, selected: Option<crate::tree::NodeId>) -> Vec<Command> {
        let mut cmds = Vec::new();
        while let Ok(e) = self.events.try_recv() {
            let cmd = match e.id().as_ref() {
                "about" => Command::About,
                "settings" => Command::OpenSettings(SettingsTab::General),
                "updates" => Command::CheckUpdates,
                "open" => Command::ChooseFolder,
                "rescan" => Command::Rescan,
                "fda" => Command::FullDiskAccess,
                "csv" => Command::ExportCsv,
                "xlsx" => Command::ExportXlsx,
                "html" => Command::ExportHtml,
                "copy_path" => match selected {
                    Some(id) => Command::Node(super::Action::CopyPath(id)),
                    None => continue,
                },
                "search" => Command::SearchView,
                "v_folders" => Command::SetView(View::Folders),
                "v_types" => Command::SetView(View::Types),
                "v_categories" => Command::SetView(View::Categories),
                "v_largest" => Command::SetView(View::Largest),
                "v_oldest" => Command::SetView(View::Oldest),
                "v_age" => Command::SetView(View::Age),
                "v_cleanup" => Command::SetView(View::Cleanup),
                "v_history" => Command::SetView(View::History),
                "v_duplicates" => Command::SetView(View::Duplicates),
                "v_cloud" => Command::SetView(View::Cloud),
                "v_apps" => Command::SetView(View::Apps),
                "c_map" => Command::SetChart(Chart::Map),
                "c_ring" => Command::SetChart(Chart::Ring),
                "shortcuts" => Command::Shortcuts,
                "log" => Command::LogFolder,
                "website" => Command::OpenWebsite,
                _ => continue,
            };
            cmds.push(cmd);
        }
        cmds
    }
}
