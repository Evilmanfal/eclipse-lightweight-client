use eframe::egui::{self, Color32, Stroke};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    Online,
    Idle,
    Dnd,
    Offline,
    #[default]
    Unknown,
}
impl Status {
    pub fn parse(s: &str) -> Self {
        match s {
            "online" => Self::Online,
            "idle" => Self::Idle,
            "dnd" => Self::Dnd,
            "offline" | "invisible" => Self::Offline,
            _ => Self::Unknown,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Online => "Online",
            Self::Idle => "Idle",
            Self::Dnd => "Do not disturb",
            Self::Offline => "Offline",
            Self::Unknown => "Status unavailable",
        }
    }
    pub fn color(self) -> Color32 {
        match self {
            Self::Online => Color32::from_rgb(69, 190, 145),
            Self::Idle => Color32::from_rgb(236, 183, 75),
            Self::Dnd => Color32::from_rgb(239, 86, 99),
            _ => Color32::from_rgb(116, 129, 153),
        }
    }
}
/// The main (non-custom-status) activity a user reports, e.g. the game they are playing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Activity {
    pub name: String,
    pub kind: u8,
    pub started_ms: Option<u64>,
    pub image: Option<String>,
}
impl Activity {
    fn parse(value: &Value) -> Option<Self> {
        let name = value["name"].as_str().filter(|n| !n.is_empty())?.chars().take(128).collect();
        let app = value["application_id"].as_str().filter(|a| crate::community::snowflake(a));
        let image = value["assets"]["large_image"].as_str().and_then(|asset| {
            if let Some(rest) = asset.strip_prefix("mp:") { Some(format!("https://media.discordapp.net/{rest}")) }
            else { app.filter(|_| asset.bytes().all(|c| c.is_ascii_alphanumeric())).map(|app| format!("https://cdn.discordapp.com/app-assets/{app}/{asset}.png")) }
        }).filter(|url| crate::assets::public_url(url));
        Some(Self { name, kind: value["type"].as_u64().unwrap_or(0) as u8, started_ms: value["timestamps"]["start"].as_u64(), image })
    }
    /// "Playing", "Streaming", "Listening to", "Watching" or "Competing in".
    pub fn verb(&self) -> &'static str { match self.kind { 1 => "Streaming", 2 => "Listening to", 3 => "Watching", 5 => "Competing in", _ => "Playing" } }
}
#[derive(Default)]
pub struct Presences {
    values: HashMap<(String, String), Status>,
    order: VecDeque<(String, String)>,
    activities: HashMap<String, Activity>,
}
impl Presences {
    pub fn get(&self, user: &str, guild: Option<&str>) -> Status {
        self.values
            .get(&(String::new(), user.into()))
            .or_else(|| self.values.get(&(guild.unwrap_or("").into(), user.into())))
            .copied()
            .unwrap_or_default()
    }
    pub fn set(&mut self, user: String, guild: String, status: Status) {
        if user.is_empty() {
            return;
        }
        let key = (guild, user);
        if !self.values.contains_key(&key) {
            self.order.push_back(key.clone());
        }
        self.values.insert(key, status);
        while self.values.len() > 4096 {
            if let Some(key) = self.order.pop_front() {
                self.values.remove(&key);
            }
        }
    }
    pub fn activity(&self, user: &str) -> Option<&Activity> { self.activities.get(user) }
    fn set_activity(&mut self, user: &str, activities: &Value, status: Option<&str>) {
        let activity = activities.as_array().into_iter().flatten().take(16).filter(|a| a["type"].as_u64() != Some(4)).find_map(Activity::parse);
        match activity.filter(|_| !matches!(status, Some("offline" | "invisible"))) {
            Some(activity) => { if self.activities.len() < 4096 || self.activities.contains_key(user) { self.activities.insert(user.to_owned(), activity); } }
            None => { self.activities.remove(user); }
        }
    }
    pub fn ingest(&mut self, data: &Value) {
        self.visit(data, "", 0);
    }
    fn visit(&mut self, data: &Value, guild: &str, depth: usize) {
        if depth > 7 {
            return;
        }
        if let Some(list) = data.as_array() {
            for item in list.iter().take(4096) {
                self.visit(item, guild, depth + 1);
            }
            return;
        }
        let guild = data["guild_id"].as_str().unwrap_or(guild);
        if let (Some(id), Some(status)) = (
            data["user"]["id"].as_str(),
            data["presence"]["status"].as_str(),
        ) {
            self.set(id.into(), guild.into(), Status::parse(status));
        }
        if let Some(status) = data["status"].as_str() {
            if let Some(id) = data["user"]["id"]
                .as_str()
                .or_else(|| data["user_id"].as_str())
            {
                self.set(id.into(), guild.into(), Status::parse(status));
                if data.get("activities").is_some() { self.set_activity(id, &data["activities"], Some(status)); }
            }
        }
        for key in [
            "presences",
            "merged_presences",
            "friends",
            "ops",
            "items",
            "member",
        ] {
            if let Some(v) = data.get(key) {
                self.visit(v, guild, depth + 1);
            }
        }
        if let Some(guilds) = data.get("guilds") {
            if let Some(list) = guilds.as_array() {
                for g in list.iter().take(1000) {
                    self.visit(g, g["id"].as_str().unwrap_or(guild), depth + 1);
                }
            }
        }
        // READY excludes offline friends; initialize those authenticated relationships only.
        if let Some(relationships) = data["relationships"].as_array() {
            for r in relationships.iter().take(4096) {
                if r["type"] == 1 {
                    if let Some(id) = r["id"].as_str().or_else(|| r["user"]["id"].as_str()) {
                        let key = (String::new(), id.to_owned());
                        if !self.values.contains_key(&key) {
                            self.set(id.into(), String::new(), Status::Offline);
                        }
                    }
                }
            }
        }
    }
}
pub fn badge(ui: &egui::Ui, rect: egui::Rect, status: Status) {
    paint_badge(ui.painter(),rect,status);
}
pub fn paint_badge(p: &egui::Painter, rect: egui::Rect, status: Status) {
    let center = rect.right_bottom() - egui::vec2(3.0, 3.0);
    let radius = (rect.width() * 0.16).clamp(4.0, 7.0);
    p.circle_filled(center, radius + 2.0, Color32::from_gray(23));
    p.circle_filled(center, radius, status.color());
    match status {
        Status::Dnd => {
            p.line_segment(
                [
                    center - egui::vec2(radius * 0.6, 0.0),
                    center + egui::vec2(radius * 0.6, 0.0),
                ],
                Stroke::new(1.8_f32, Color32::from_gray(23)),
            );
        }
        Status::Idle => {
            p.circle_filled(
                center - egui::vec2(radius * 0.35, radius * 0.35),
                radius * 0.75,
                Color32::from_gray(23),
            );
        }
        Status::Offline | Status::Unknown => {
            p.circle_filled(center, radius * 0.5, Color32::from_gray(23));
        }
        _ => {}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshots_updates_and_unknown() {
        let mut p = Presences::default();
        p.ingest(&serde_json::json!({"merged_presences":{"friends":[{"user_id":"1","status":"idle"}],"guilds":[[{"user_id":"2","guild_id":"9","status":"dnd"}]]},"relationships":[{"id":"3","type":1}]}));
        assert_eq!(p.get("1", None), Status::Idle);
        assert_eq!(p.get("2", Some("9")), Status::Dnd);
        assert_eq!(p.get("3", None), Status::Offline);
        assert_eq!(p.get("4", None), Status::Unknown);
        p.ingest(&serde_json::json!({"user":{"id":"1"},"status":"online"}));
        p.ingest(&serde_json::json!({"user":{"id":"5"},"status":"dnd","activities":[{"type":4,"name":"Custom Status"},{"type":0,"name":"ROBLOX","timestamps":{"start":1000}}]}));
        assert_eq!(p.activity("5").map(|a|(a.name.as_str(),a.verb(),a.started_ms)),Some(("ROBLOX","Playing",Some(1000))));
        p.ingest(&serde_json::json!({"user":{"id":"5"},"status":"offline","activities":[]}));assert!(p.activity("5").is_none());
        assert_eq!(p.get("1", None), Status::Online);
    }
}
