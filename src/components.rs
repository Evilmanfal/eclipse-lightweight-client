//! Message components sent by apps and bots: action rows with buttons and select menus, and the
//! Components V2 layouts (containers, sections, text displays, separators, media galleries) that
//! replace a message's text entirely.
use crate::assets::Images;
use crate::model::Message;
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use serde_json::Value;

const TEXT: Color32 = Color32::from_gray(225);
const MUTED: Color32 = Color32::from_gray(160);
/// Layouts nest (container → section → button); deeper trees are ignored.
const MAX_DEPTH: u8 = 5;

/// "name used /command" for a message sent in reply to a slash command.
pub fn command_use(message: &Message) -> Option<(String, String)> {
    let source = message.interaction_metadata.as_ref().filter(|m| m["name"].is_string()).or(message.interaction.as_ref())?;
    let name = source["name"].as_str()?.to_owned();
    let user = &source["user"];
    let who = user["global_name"].as_str().or(user["username"].as_str()).unwrap_or("Someone").to_owned();
    Some((who, name))
}

/// Draws every top-level component of a message.
pub fn show(ui: &mut egui::Ui, images: &mut Images, components: &[Value]) {
    if components.is_empty() { return; }
    ui.add_space(4.0);
    ui.vertical(|ui| {
        ui.set_max_width(ui.available_width().min(560.0));
        ui.spacing_mut().item_spacing.y = 8.0;
        for (index, component) in components.iter().take(40).enumerate() { ui.push_id(index, |ui| show_one(ui, images, component, 0)); }
    });
}

fn children(component: &Value, key: &str) -> Vec<Value> { component[key].as_array().map(|a| a.iter().take(40).cloned().collect()).unwrap_or_default() }

fn show_one(ui: &mut egui::Ui, images: &mut Images, component: &Value, depth: u8) {
    if depth > MAX_DEPTH { return; }
    match component["type"].as_u64().unwrap_or(0) {
        // Action row
        1 => { ui.horizontal_wrapped(|ui| { ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0); for (i, child) in children(component, "components").iter().enumerate() { ui.push_id(i, |ui| show_one(ui, images, child, depth + 1)); } }); }
        2 => button(ui, component),
        3 | 5 | 6 | 7 | 8 => select(ui, component),
        // Section: text on the left, a thumbnail or button on the right.
        9 => {
            let accessory = component.get("accessory").filter(|a| a.is_object());
            ui.horizontal_top(|ui| {
                let reserve = if accessory.is_some() { 96.0 } else { 0.0 };
                ui.vertical(|ui| { ui.set_width((ui.available_width() - reserve).max(120.0)); ui.spacing_mut().item_spacing.y = 4.0; for (i, child) in children(component, "components").iter().enumerate() { ui.push_id(i, |ui| show_one(ui, images, child, depth + 1)); } });
                if let Some(accessory) = accessory { ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| show_one(ui, images, accessory, depth + 1)); }
            });
        }
        10 => if let Some(text) = component["content"].as_str() { text_display(ui, images, text); },
        // Thumbnail
        11 => if let Some(url) = component["media"]["url"].as_str() { ui.scope(|ui| { ui.set_max_width(80.0); picture(ui, images, &component["media"], url); }); },
        // Media gallery
        12 => { ui.horizontal_wrapped(|ui| { for (i, item) in children(component, "items").iter().take(10).enumerate() { if let Some(url) = item["media"]["url"].as_str() { ui.push_id(i, |ui| { ui.scope(|ui| { ui.set_max_width(if i == 0 { 360.0 } else { 170.0 }); picture(ui, images, &item["media"], url); }); }); } } }); }
        // File
        13 => {
            let name = component["name"].as_str().or_else(|| component["file"]["url"].as_str().and_then(|u| u.rsplit('/').next())).unwrap_or("File");
            egui::Frame::NONE.fill(Color32::from_gray(24)).stroke(Stroke::new(1.0_f32, Color32::from_gray(48))).corner_radius(8).inner_margin(10).show(ui, |ui| { ui.label(RichText::new(format!("📄  {name}")).color(TEXT)); });
        }
        // Separator
        14 => {
            let gap = if component["spacing"].as_u64() == Some(2) { 12.0 } else { 4.0 };
            ui.add_space(gap);
            if component["divider"].as_bool() != Some(false) { let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover()); ui.painter().rect_filled(rect, 0, Color32::from_gray(52)); }
            ui.add_space(gap);
        }
        // Container: a card with an optional accent bar.
        17 => {
            let accent = component["accent_color"].as_u64().map(|c| Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8));
            let response = egui::Frame::NONE.fill(Color32::from_gray(27)).stroke(Stroke::new(1.0_f32, Color32::from_gray(44))).corner_radius(8)
                .inner_margin(egui::Margin { left: if accent.is_some() { 18 } else { 14 }, right: 14, top: 12, bottom: 12 })
                .show(ui, |ui| { ui.set_width(ui.available_width()); ui.spacing_mut().item_spacing.y = 8.0; for (i, child) in children(component, "components").iter().enumerate() { ui.push_id(i, |ui| show_one(ui, images, child, depth + 1)); } }).response;
            if let Some(accent) = accent { let rect = response.rect; ui.painter().rect_filled(egui::Rect::from_min_max(rect.min, egui::pos2(rect.left() + 4.0, rect.bottom())), egui::CornerRadius { nw: 8, sw: 8, ne: 0, se: 0 }, accent); }
        }
        _ => {}
    }
}

fn picture(ui: &mut egui::Ui, images: &mut Images, media: &Value, url: &str) {
    let size = |key: &str| media[key].as_u64().map(|n| n.min(4096) as u32);
    let gif = url.contains(".gif") || media["content_type"].as_str().is_some_and(|t| t.contains("gif"));
    crate::message_media::picture(ui, images, url, size("width"), size("height"), gif);
}

/// Markdown-ish text: # / ## / ### headings and -# subtext get their own sizes; other lines are
/// regular message text with emoji and links.
fn text_display(ui: &mut egui::Ui, images: &mut Images, text: &str) {
    let mut plain = String::new();
    let flush = |ui: &mut egui::Ui, images: &mut Images, plain: &mut String| { let body = plain.trim_end_matches('\n'); if !body.is_empty() { crate::message_media::body(ui, images, body); } plain.clear(); };
    for line in text.lines().take(200) {
        let heading = [("### ", 15.5), ("## ", 17.5), ("# ", 20.0)].into_iter().find(|(prefix, _)| line.starts_with(prefix));
        if let Some((prefix, size)) = heading {
            flush(ui, images, &mut plain);
            ui.label(RichText::new(strip_bold(&line[prefix.len()..])).size(size).strong().color(Color32::WHITE));
        } else if let Some(small) = line.strip_prefix("-# ") {
            flush(ui, images, &mut plain);
            ui.label(RichText::new(strip_bold(small)).size(11.5).color(MUTED));
        } else { plain.push_str(&strip_bold(line)); plain.push('\n'); }
    }
    flush(ui, images, &mut plain);
}
fn strip_bold(text: &str) -> String { text.replace("**", "").replace("__", "") }

fn emoji_prefix(component: &Value) -> String {
    match (component["emoji"]["name"].as_str(), component["emoji"]["id"].as_str()) { (Some(name), None) => format!("{name} "), _ => String::new() }
}

fn button(ui: &mut egui::Ui, component: &Value) {
    let style = component["style"].as_u64().unwrap_or(2);
    let fill = match style { 1 | 6 => Color32::from_rgb(88, 101, 242), 3 => Color32::from_rgb(36, 128, 70), 4 => Color32::from_rgb(218, 55, 60), _ => Color32::from_gray(70) };
    let disabled = component["disabled"].as_bool() == Some(true);
    let label = format!("{}{}{}", emoji_prefix(component), component["label"].as_str().unwrap_or(""), if style == 5 { "  ↗" } else { "" });
    let label = if label.trim().is_empty() { "•".to_owned() } else { label };
    let fill = if disabled { fill.gamma_multiply(0.45) } else { fill };
    let text = if disabled { Color32::from_gray(170) } else { Color32::WHITE };
    let response = ui.add(egui::Button::new(RichText::new(label.trim()).color(text)).fill(fill).stroke(Stroke::NONE).corner_radius(8).min_size(Vec2::new(60.0, 32.0)));
    match component["url"].as_str().filter(|_| style == 5 && !disabled) {
        Some(url) if crate::model::safe_link(url) => { if response.on_hover_text(url).clicked() { ui.ctx().open_url(egui::OpenUrl::new_tab(url)); } }
        _ => { response.on_hover_text(if disabled { "This button is disabled." } else { "Use this button in Discord. Eclipse can't send button presses to apps yet." }); }
    }
}

fn select(ui: &mut egui::Ui, component: &Value) {
    let options = children(component, "options");
    let chosen: Vec<String> = options.iter().filter(|o| o["default"].as_bool() == Some(true)).map(|o| format!("{}{}", emoji_prefix(o), o["label"].as_str().unwrap_or(""))).collect();
    let text = if chosen.is_empty() { component["placeholder"].as_str().unwrap_or("Make a selection").to_owned() } else { chosen.join(", ") };
    let disabled = component["disabled"].as_bool() == Some(true);
    egui::Frame::NONE.fill(Color32::from_gray(22)).stroke(Stroke::new(1.0_f32, Color32::from_gray(50))).corner_radius(8).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width().min(400.0));
        ui.horizontal(|ui| {
            ui.label(RichText::new(text).color(if chosen.is_empty() || disabled { MUTED } else { TEXT }));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { ui.label(RichText::new("⌄").color(MUTED)); });
        });
    }).response.on_hover_text("Use this menu in Discord. Eclipse can't send menu choices to apps yet.");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn components_v2_container_renders_text_buttons_and_menus() {
        let message: Message = serde_json::from_value(serde_json::json!({
            "id": "1", "channel_id": "2", "content": "", "flags": 32768,
            "interaction_metadata": {"name": "config", "user": {"id": "3", "username": "husain"}},
            "components": [{"type": 17, "accent_color": 5793266, "components": [
                {"type": 10, "content": "# Autofix Configuration\nConfigure which social platforms have link fixing enabled."},
                {"type": 14, "divider": true, "spacing": 1},
                {"type": 1, "components": [{"type": 3, "custom_id": "a", "placeholder": "Autofix", "options": [{"label": "X", "value": "x", "default": true, "emoji": {"name": "🟢"}}]}]},
                {"type": 1, "components": [{"type": 2, "style": 4, "label": "Disable X", "custom_id": "b"}]}
            ]}]
        })).unwrap();
        assert_eq!(command_use(&message), Some(("husain".into(), "config".into())));
        let ctx = egui::Context::default();
        let mut images = Images::new(&ctx);
        let output = ctx.run(Default::default(), |ctx| { egui::CentralPanel::default().show(ctx, |ui| show(ui, &mut images, &message.components)); });
        let texts: Vec<String> = output.shapes.iter().filter_map(|s| match &s.shape { egui::Shape::Text(t) => Some(t.galley.job.text.clone()), _ => None }).collect();
        for expected in ["Autofix Configuration", "Configure which social platforms have link fixing enabled.", "🟢 X", "Disable X"] {
            assert!(texts.iter().any(|t| t == expected), "missing {expected}: {texts:?}");
        }
    }
}
