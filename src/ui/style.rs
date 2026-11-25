//! Fonts, text size and contrast.
//!
//! The built-in egui font is very thin. The program uses the font of the
//! system instead (Segoe UI, Helvetica Neue, Noto Sans or DejaVu Sans); the
//! egui fonts stay as fallback for symbols. Text is a little darker (light
//! mode) or brighter (dark mode) than egui's default.

use std::sync::Arc;

use eframe::egui;
use egui::{Color32, FontData, FontDefinitions, FontFamily, Theme};

/// Text sizes offered in the settings (percent).
pub const ZOOMS: [u16; 6] = [90, 100, 110, 125, 140, 160];

/// Font files of the system, best first: (path, index in a collection).
fn candidates() -> Vec<(std::path::PathBuf, u32)> {
    let mut v = Vec::new();
    if cfg!(windows) {
        let dir = std::env::var_os("WINDIR").map_or_else(|| "C:\\Windows".into(), std::path::PathBuf::from);
        for f in ["segoeui.ttf", "arial.ttf"] {
            v.push((dir.join("Fonts").join(f), 0));
        }
    } else if cfg!(target_os = "macos") {
        for f in [
            "/System/Library/Fonts/HelveticaNeue.ttc",
            "/System/Library/Fonts/Helvetica.ttc",
            "/System/Library/Fonts/Supplemental/Arial.ttf",
        ] {
            v.push((f.into(), 0));
        }
    } else {
        for f in [
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        ] {
            v.push((f.into(), 0));
        }
    }
    v
}

/// The fonts: the system font first, egui's own fonts behind it.
pub fn fonts() -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    // Font files over 40 MB are not taken (a normal font has a few MB).
    let found = candidates().into_iter().find_map(|(path, index)| {
        let len = std::fs::metadata(&path).ok()?.len();
        if len > 40 << 20 {
            return None;
        }
        std::fs::read(&path).ok().map(|bytes| (bytes, index))
    });
    if let Some((bytes, index)) = found {
        let data = FontData::from_blob(Arc::new(bytes), index);
        defs.font_data.insert("system-ui".to_owned(), Arc::new(data));
        if let Some(family) = defs.families.get_mut(&FontFamily::Proportional) {
            family.insert(0, "system-ui".to_owned());
        }
        if let Some(family) = defs.families.get_mut(&FontFamily::Monospace) {
            family.push("system-ui".to_owned());
        }
    }
    defs
}

/// Applies fonts, contrast and text size.
pub fn apply(ctx: &egui::Context, zoom_percent: u16) {
    ctx.set_fonts(fonts());
    ctx.style_mut_of(Theme::Light, |s| {
        s.visuals.widgets.noninteractive.fg_stroke.color = Color32::from_gray(25);
        s.visuals.widgets.inactive.fg_stroke.color = Color32::from_gray(20);
    });
    ctx.style_mut_of(Theme::Dark, |s| {
        s.visuals.widgets.noninteractive.fg_stroke.color = Color32::from_gray(225);
        s.visuals.widgets.inactive.fg_stroke.color = Color32::from_gray(235);
    });
    ctx.all_styles_mut(|s| {
        // Slightly larger base sizes than egui's defaults.
        for (style, size) in [
            (egui::TextStyle::Body, 14.0),
            (egui::TextStyle::Button, 14.0),
            (egui::TextStyle::Small, 11.5),
            (egui::TextStyle::Heading, 19.0),
            (egui::TextStyle::Monospace, 13.0),
        ] {
            if let Some(f) = s.text_styles.get_mut(&style) {
                f.size = size;
            }
        }
    });
    ctx.set_zoom_factor(f32::from(zoom_percent.clamp(50, 300)) / 100.0);
}

#[cfg(test)]
mod tests {
    #[test]
    fn fonts_always_have_a_proportional_family() {
        let defs = super::fonts();
        let family = &defs.families[&egui::FontFamily::Proportional];
        assert!(!family.is_empty());
        assert!(family.iter().all(|name| defs.font_data.contains_key(name)));
    }

    use eframe::egui;
}
