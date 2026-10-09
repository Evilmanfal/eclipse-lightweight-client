//! Portable preferences contain presentation/device choices only, never account credentials.
use eframe::egui::{self,Color32};
use serde::{Deserialize,Serialize};
use std::{collections::HashMap,path::PathBuf};
#[derive(Clone,Serialize,Deserialize,PartialEq)]
#[serde(default)]
pub struct Preferences {
    pub media_revision:u32,pub ui_sounds:bool,pub compact:bool,pub members:bool,pub animations:bool,pub images:bool,
    pub developer:bool,pub zoom:f32,pub font_size:f32,pub animation_fps:u32,
    pub idle_seconds:u64,pub show_usernames:bool,pub character_count:bool,
    pub role_colors:bool,pub member_count:bool,pub quiet_mentions:bool,
    pub reduced_motion:bool,pub contrast:bool,pub streamer:bool,
    pub spotify:bool,pub volume_booster:bool,pub silent_typing:bool,pub read_all:bool,pub message_logger:bool,pub click_actions:bool,pub activity_toggle:bool,
    pub input:Option<String>,pub output:Option<String>,pub input_gain:u16,pub output_gain:u16,
    pub processing:voice_model::voice_settings::VoiceProcessing,
    /// Upload a preview picture of your screen share for others (Screen Share dialog).
    pub stream_preview:bool,pub push_to_talk:bool,pub ptt_key:u32,pub ptt_ctrl:bool,pub ptt_shift:bool,pub ptt_alt:bool,pub screen_height:u32,pub screen_fps:u32,
    pub theme:Theme,pub aliases:HashMap<String,String>,pub plugins:Vec<Plugin>,
    /// Height the pinned-messages dropdown was last dragged to.
    pub pins_height:f32,
}
impl Default for Preferences {fn default()->Self{Self{pins_height:360.0,media_revision:1,ui_sounds:true,compact:false,members:true,animations:true,images:true,developer:false,zoom:1.0,font_size:15.0,animation_fps:60,idle_seconds:2,show_usernames:false,character_count:true,role_colors:true,member_count:true,quiet_mentions:false,reduced_motion:false,contrast:false,streamer:false,spotify:true,volume_booster:true,silent_typing:true,read_all:true,message_logger:true,click_actions:true,activity_toggle:true,input:None,output:None,input_gain:100,output_gain:100,processing:Default::default(),stream_preview:true,push_to_talk:false,ptt_key:119,ptt_ctrl:false,ptt_shift:false,ptt_alt:false,screen_height:720,screen_fps:30,theme:Theme::default(),aliases:HashMap::new(),plugins:vec![]}}}
#[derive(Clone,Serialize,Deserialize,PartialEq)]
#[serde(default)]pub struct Theme{pub name:String,pub background:String,pub surface:String,pub accent:String,pub text:String}
impl Default for Theme{fn default()->Self{Self{name:"Material Black".into(),background:"#0b0b0b".into(),surface:"#171717".into(),accent:"#b8b8b8".into(),text:"#ededed".into()}}}
impl Theme {
    pub fn presets()->Vec<Self>{vec![Self::default(),Self{name:"Discord dark".into(),background:"#313338".into(),surface:"#2b2d31".into(),accent:"#5865f2".into(),text:"#dbdee1".into()},Self{name:"Midnight".into(),background:"#111214".into(),surface:"#0b0c0e".into(),accent:"#8b9cff".into(),text:"#d8dee9".into()},Self{name:"Forest".into(),background:"#192522".into(),surface:"#111d19".into(),accent:"#8ce3bf".into(),text:"#d0e4db".into()}]}
    pub fn valid(&self)->bool{[&self.background,&self.surface,&self.accent,&self.text].iter().all(|s|color(s).is_some())}
    pub fn import(text:&str)->Result<Self,String>{
        if text.len()>256*1024{return Err("Theme file is too large.".into());}
        if let Ok(theme)=serde_json::from_str::<Self>(text){if theme.valid(){return Ok(theme);}}
        let mut theme=Self{name:"Imported CSS colors".into(),..Default::default()};let mut found=0;
        // Read literal color variables only. CSS selectors, scripts and @imports are not executed.
        for (target,keys) in [(&mut theme.background,vec!["--background-primary","--background-base-lowest"]),(&mut theme.surface,vec!["--background-secondary","--background-base-lower"]),(&mut theme.accent,vec!["--brand-experiment","--brand-500","--accent-color"]),(&mut theme.text,vec!["--text-normal","--text-default"])]{
            for key in keys {if let Some((_,rest))=text.rsplit_once(&format!("{key}:")){let value=rest.split(';').next().unwrap_or("").trim();if color(value).is_some(){*target=value.into();found+=1;break;}}}
        }
        if found==0{Err("No supported literal CSS color variables were found. Import a native JSON theme or #RRGGBB Discord color variables.".into())}else{Ok(theme)}
    }
}
pub fn color(s:&str)->Option<Color32>{let s=s.strip_prefix('#')?;if s.len()!=6{return None;}let n=u32::from_str_radix(s,16).ok()?;Some(Color32::from_rgb((n>>16)as u8,(n>>8)as u8,n as u8))}
#[derive(Clone,Default,Serialize,Deserialize,PartialEq)]
#[serde(default)]pub struct Plugin{pub name:String,pub description:String,pub enabled:bool,pub replacements:Vec<Replacement>}
#[derive(Clone,Default,Serialize,Deserialize,PartialEq)]pub struct Replacement{pub find:String,pub display:String}
impl Plugin {
    pub fn parse(text:&str)->Result<Self,String>{if text.len()>64*1024{return Err("Plugin exceeds 64 KiB.".into());}let p:Self=serde_json::from_str(text).map_err(|_|"Use a Eclipse .json display plugin. Vencord JavaScript plugins require Discord's web runtime.")?;if p.name.is_empty()||p.name.len()>80||p.replacements.len()>32||p.replacements.iter().any(|r|r.find.is_empty()||r.find.len()>100||r.display.len()>200){return Err("Invalid plugin name or display replacement rules.".into());}Ok(p)}
}
impl Preferences {
    pub fn path()->PathBuf {std::env::current_exe().ok().and_then(|p|p.parent().map(|p|p.join("eclipse-settings.json"))).unwrap_or_else(||PathBuf::from("eclipse-settings.json"))}
    pub fn load()->Self{let path=Self::path();let path=if !path.exists(){let legacy=path.with_file_name("feather-settings.json");if legacy.exists(){legacy}else{path}}else{path};if std::fs::metadata(&path).is_ok_and(|m|m.len()<=256*1024){if let Ok(bytes)=std::fs::read(path){if let Ok(mut p)=serde_json::from_slice::<Self>(&bytes){let revision=serde_json::from_slice::<serde_json::Value>(&bytes).ok().and_then(|v|v["media_revision"].as_u64()).unwrap_or(0);if revision==0&&p.animation_fps==30{p.animation_fps=60;}p.media_revision=1;p.normalize();return p;}}}Self::default()}
    pub fn save(&self)->Result<(),String>{let path=Self::path();let bytes=serde_json::to_vec_pretty(self).map_err(|_|"Could not serialize settings.")?;let temporary=path.with_extension("json.tmp");std::fs::write(&temporary,bytes).map_err(|_|"Could not save preferences beside Eclipse.exe.")?;std::fs::rename(temporary,path).map_err(|_|"Could not replace the preferences file.".into())}
    pub fn normalize(&mut self){self.pins_height=if self.pins_height.is_finite(){self.pins_height.clamp(140.0,1400.0)}else{360.0};self.zoom=if self.zoom.is_finite(){self.zoom.clamp(0.75,1.5)}else{1.0};self.font_size=if self.font_size.is_finite(){self.font_size.clamp(12.0,24.0)}else{15.0};self.animation_fps=self.animation_fps.clamp(5,60);self.idle_seconds=self.idle_seconds.clamp(1,10);if !(1..=254).contains(&self.ptt_key){self.ptt_key=119;}self.input_gain=self.input_gain.min(200);self.output_gain=self.output_gain.min(200);if !self.processing.effective().is_valid(){self.processing=Default::default();}if !self.theme.valid() || (self.theme.name=="Charcoal & cyan" && self.theme.background=="#1e2028" && self.theme.surface=="#16181e" && self.theme.accent=="#41b7cd"){self.theme=Theme::default();}self.plugins.truncate(32);self.plugins.retain(|p|Plugin::parse(&serde_json::to_string(p).unwrap_or_default()).is_ok());self.aliases.retain(|k,v|k.len()<=20&&v.len()<=80);if self.aliases.len()>1000{self.aliases.clear();}if ![720,1080].contains(&self.screen_height){self.screen_height=720;}if ![15,30,60].contains(&self.screen_fps){self.screen_fps=30;}}
    pub fn display(&self,text:&str)->String{let mut display=text.to_owned();for p in self.plugins.iter().filter(|p|p.enabled){for r in &p.replacements{display=display.replacen(&r.find,&r.display,100);if display.len()>64*1024{display.truncate(display.floor_char_boundary(64*1024));return display;}}}display}
    pub fn apply(&self, ctx:&egui::Context) {
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new("eclipse-name-animation"), self.animations&&!self.reduced_motion);
            d.insert_temp(egui::Id::new("eclipse-animation-fps"), self.animation_fps);
        });
        ctx.set_zoom_factor(self.zoom*if self.compact{COMPACT_SCALE}else{1.0});
        let mut style = (*ctx.style()).clone();
        let background = color(&self.theme.background).unwrap_or(Color32::from_gray(11));
        let surface = color(&self.theme.surface).unwrap_or(Color32::from_gray(23));
        let accent = color(&self.theme.accent).unwrap_or(Color32::from_gray(184));
        let text = color(&self.theme.text).unwrap_or(Color32::from_gray(237));
        let card = surface.lerp_to_gamma(Color32::WHITE, 0.055);
        style.visuals.panel_fill = background;
        style.visuals.extreme_bg_color = surface;
        style.visuals.window_fill = card;
        style.visuals.faint_bg_color = card;
        style.visuals.override_text_color = Some(text);
        style.visuals.hyperlink_color = accent;
        style.visuals.selection.bg_fill = accent.lerp_to_gamma(surface, 0.75);
        style.visuals.selection.stroke = egui::Stroke::new(1_f32, accent);
        style.visuals.window_corner_radius = egui::CornerRadius::same(18);
        style.visuals.menu_corner_radius = egui::CornerRadius::same(12);
        style.visuals.window_stroke = egui::Stroke::new(1_f32, surface.lerp_to_gamma(Color32::WHITE, 0.12));
        for visuals in [&mut style.visuals.widgets.inactive, &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active, &mut style.visuals.widgets.open] {
            visuals.corner_radius = egui::CornerRadius::same(18);
            visuals.bg_stroke = egui::Stroke::NONE;
            visuals.fg_stroke.color = text;
        }
        // Slider rails and checkbox fills need to stand apart from the surrounding card.
        style.visuals.widgets.inactive.bg_fill = surface.lerp_to_gamma(accent, 0.30);
        style.visuals.widgets.inactive.weak_bg_fill = card;
        let hovered = surface.lerp_to_gamma(accent, 0.19);
        style.visuals.widgets.hovered.bg_fill = hovered;
        style.visuals.widgets.hovered.weak_bg_fill = hovered;
        style.visuals.widgets.active.bg_fill = style.visuals.selection.bg_fill;
        style.visuals.widgets.active.weak_bg_fill = style.visuals.selection.bg_fill;
        style.visuals.widgets.open.bg_fill = hovered;
        style.visuals.widgets.open.weak_bg_fill = hovered;
        style.visuals.widgets.noninteractive.bg_stroke.color = surface.lerp_to_gamma(Color32::WHITE, 0.10);
        style.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(self.font_size));
        style.animation_time = if self.reduced_motion { 0.0 } else { 0.12 };
        if self.contrast { style.visuals.override_text_color = Some(Color32::WHITE); }
        ctx.set_style(style);
    }
}
/// Compact mode draws the whole interface at this fraction of the chosen UI scale.
pub const COMPACT_SCALE:f32=0.85;
pub const USER_CATEGORIES:&[(&str,&[&str])]=&[
    ("YOUR ACCOUNT", &["Account & Profile","Connections","Devices","Authorized Apps"]),
    ("PRIVACY & SAFETY", &["Content & Social","Data & Privacy","Family Center"]),
    ("BILLING & NITRO", &["Nitro","Server Boost","Subscriptions","Gift Inventory","Billing"]),
    ("APP SETTINGS", &["Appearance","Accessibility","Voice & Video","Chat","Notifications","Keybinds","Language","Streamer Mode","Advanced"]),
    ("ACTIVITY", &["Activity Privacy","Registered Games"]),
    ("ECLIPSE", &["Plugins","Themes","Performance"]),
];
#[cfg(test)]mod tests{use super::*;
#[test]fn imports_only_literal_theme_colors(){let t=Theme::import(":root { --background-primary: #010203; --brand-500: #123456; } @import 'evil';").unwrap();assert_eq!(t.background,"#010203");assert_eq!(t.accent,"#123456");assert!(Theme::import("@import 'evil';").is_err());}
#[test]fn display_plugins_cannot_change_original_message(){let p=Plugin::parse(r#"{"name":"Local display","enabled":true,"replacements":[{"find":"hello","display":"hi"}]}"#).unwrap();let mut prefs=Preferences::default();prefs.plugins.push(p);let original="hello world";assert_eq!(prefs.display(original),"hi world");assert_eq!(original,"hello world");assert!(Plugin::parse(r#"{"name":"Bad","replacements":[{"find":"","display":"x"}]}"#).is_err());}
#[test]fn compact_mode_zooms_the_interface_out_from_the_chosen_scale(){let ctx=egui::Context::default();let mut p=Preferences::default();p.zoom=1.2;let zoom=|p:&Preferences|{p.apply(&ctx);let _=ctx.run(Default::default(),|_|{});ctx.zoom_factor()};assert!((zoom(&p)-1.2).abs()<0.001);p.compact=true;assert!((zoom(&p)-1.2*COMPACT_SCALE).abs()<0.001);}
#[test]fn preferences_are_bounded_and_contain_no_credentials(){let mut p=Preferences::default();p.font_size=999.;p.animation_fps=999;p.input_gain=999;p.normalize();assert_eq!(p.font_size,24.);assert_eq!(p.animation_fps,60);assert_eq!(p.input_gain,200);let v=serde_json::to_value(p).unwrap();assert!(v.get("token").is_none());}
#[test] fn legacy_theme_upgrade_keeps_hotkeys_and_custom_palettes() {
    let mut p:Preferences=serde_json::from_str(r##"{"theme":{"name":"Charcoal & cyan","background":"#1e2028","surface":"#16181e","accent":"#41b7cd","text":"#b2bed5"},"push_to_talk":true,"ptt_key":65,"ptt_ctrl":true,"animation_fps":24,"ui_sounds":false}"##).unwrap();
    p.normalize();
    assert_eq!(p.theme.name,"Material Black");
    assert!(p.push_to_talk&&p.ptt_ctrl);
    assert_eq!(p.ptt_key,65);assert_eq!(p.animation_fps,24);assert!(!p.ui_sounds);
    p.theme=Theme{name:"Custom".into(),accent:"#123456".into(),..Theme::default()};
    p.normalize();assert_eq!(p.theme.accent,"#123456");
}

}

pub const SERVER_CATEGORIES:&[(&str,&[&str])]=&[
 ("SERVER",&["Overview","Roles","Members","Channels","Notifications","Boost Status"]),
 ("EXPRESSION",&["Emoji","Stickers","Soundboard"]),
 ("MODERATION",&["AutoMod","Bans","Audit Log"]),
 ("COMMUNITY",&["Community","Onboarding","Welcome Screen","Invites"]),
 ("APPS & SHARING",&["Integrations","Webhooks","Server Widget","Server Template","Vanity URL"]),
];
