use crate::{assets::Images, backend::Command};
use eframe::egui::{self, Vec2};
use serde::Deserialize;
#[derive(Clone, Debug, Default, Deserialize)]
pub struct CustomEmoji {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub animated: bool,
    #[serde(default)]
    pub available: Option<bool>,
}
impl CustomEmoji {
    pub fn token(&self) -> String {
        format!(
            "<{}:{}:{}>",
            if self.animated { "a" } else { "" },
            self.name,
            self.id
        )
    }
    pub fn image(&self) -> Option<String> {
        emoji_url(&self.id, self.animated)
    }
}
pub fn emoji_url(id: &str, animated: bool) -> Option<String> {
    (!id.is_empty() && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit())).then(|| {
        format!(
            "https://cdn.discordapp.com/emojis/{id}.{}?size=64",
            if animated { "gif" } else { "png" }
        )
    })
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Gif {
    #[serde(default)]
    pub url: String,
    pub src: Option<String>,
    pub gif_src: Option<String>,
    pub preview: Option<String>,
    #[serde(default)]
    pub title: String,
}
impl Gif {
    pub fn image(&self) -> &str {
        self.gif_src.as_deref().filter(|url|crate::assets::public_url(url))
            .or(self.src.as_deref().filter(|url|crate::assets::public_url(url)))
            .or(self.preview.as_deref())
            .unwrap_or("")
    }
    pub fn share(&self) -> &str {
        [self.gif_src.as_deref(),self.src.as_deref(),self.preview.as_deref()].into_iter().flatten()
            .find(|url| crate::assets::public_url(url))
            .unwrap_or(&self.url)
    }
}
#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Emoji,
    Gif,
}
#[derive(Default)]
pub struct Picker {
    pub mode: Option<Mode>,
    pub channel: String,
    pub guild: Option<String>,
    pub emojis: Vec<CustomEmoji>,
    /// Nitro: emojis from your other servers, by server name, usable anywhere.
    pub other_servers: Vec<(String, Vec<CustomEmoji>)>,
    pub gifs: Vec<Gif>,
    /// The GIF home: (label, preview picture), Trending GIFs first.
    pub categories: Vec<(String, String)>,
    categories_pending: bool,
    pub query: String,
    pub requested: String,
    pub pending: bool,
    pub anchor: Option<egui::Rect>,
    just_opened: bool,
    pub popup_rect: Option<egui::Rect>,
}
pub const EMOJIS: &[(&str, &str)] = &[
    ("😀", "smile grinning"),
    ("😃", "happy"),
    ("😄", "laugh happy"),
    ("😁", "grin"),
    ("😆", "laugh"),
    ("😅", "sweat smile"),
    ("😂", "laugh tears"),
    ("🤣", "rolling laugh"),
    ("😊", "blush smile"),
    ("🙂", "smile"),
    ("🙃", "upside down"),
    ("😉", "wink"),
    ("😍", "love heart eyes"),
    ("🥰", "love hearts"),
    ("😘", "kiss"),
    ("😎", "cool sunglasses"),
    ("🥳", "party"),
    ("🤔", "think"),
    ("🫡", "salute"),
    ("🤗", "hug"),
    ("😴", "sleep"),
    ("😢", "cry"),
    ("😭", "cry sob"),
    ("😡", "angry"),
    ("🤯", "mind blown"),
    ("😱", "scream"),
    ("💀", "skull"),
    ("👀", "eyes"),
    ("👍", "thumbs up yes"),
    ("👎", "thumbs down"),
    ("👏", "clap"),
    ("🙌", "celebrate hands"),
    ("👋", "wave hello"),
    ("🤝", "handshake"),
    ("🙏", "pray please thanks"),
    ("💪", "strong muscle"),
    ("❤️", "heart love"),
    ("💜", "purple heart"),
    ("💙", "blue heart"),
    ("💚", "green heart"),
    ("💔", "broken heart"),
    ("✨", "sparkles"),
    ("🔥", "fire"),
    ("🎉", "party celebrate"),
    ("🎊", "confetti"),
    ("✅", "check yes"),
    ("❌", "cross no"),
    ("⚠️", "warning"),
    ("💯", "hundred"),
    ("⭐", "star"),
    ("🌈", "rainbow"),
    ("☀️", "sun"),
    ("🌙", "moon"),
    ("🐱", "cat"),
    ("🐶", "dog"),
    ("🦁", "lion"),
    ("🐾", "paws"),
    ("🌸", "flower"),
    ("☕", "coffee"),
    ("🍕", "pizza"),
    ("🎮", "game"),
    ("🎵", "music"),
    ("🚀", "rocket"),
    ("💻", "computer"),
];
pub fn unicode_url(emoji: &str) -> String {
    let name = emoji
        .chars()
        .filter(|c| *c != '\u{fe0f}')
        .map(|c| format!("{:x}", c as u32))
        .collect::<Vec<_>>()
        .join("-");
    format!("https://cdnjs.cloudflare.com/ajax/libs/twemoji/14.0.2/72x72/{name}.png")
}
impl Picker {
    pub fn open(&mut self, mode: Mode, channel: &str, guild: Option<&str>, anchor: egui::Rect) -> Option<Command> {
        if self.mode == Some(mode) && self.channel == channel {
            self.close();
            return None;
        }
        self.mode = Some(mode);
        self.anchor = Some(anchor);
        self.just_opened = true;
        self.channel = channel.into();
        self.query.clear();
        if mode == Mode::Gif && self.categories.is_empty() && !self.categories_pending {
            self.categories_pending = true;
            return Some(Command::GifCategories);
        }
        if mode == Mode::Emoji && self.guild.as_deref() != guild {
            self.guild = guild.map(str::to_owned);
            self.emojis.clear();
            return guild.map(|g| Command::Emojis(g.into()));
        }
        None
    }
    pub fn set_categories(&mut self, categories: Vec<(String, String)>) {
        self.categories = categories;
        self.categories_pending = false;
    }
    pub fn close(&mut self) {
        self.mode = None;
        self.anchor = None;
        self.popup_rect = None;
    }
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        images: &mut Images,
    ) -> (Option<(String, String)>, Option<Command>) {
        let Some(mode) = self.mode else {
            return (None, None);
        };
        let Some(anchor) = self.anchor else { self.close(); return (None, None); };
        let just_opened = std::mem::take(&mut self.just_opened);
        let mut open = true;
        let mut picked = None;
        let mut command = None;
        // One fixed size for both tabs: the panel no longer shrinks to an empty GIF list
        // (egui then remembered the small size for Emoji too).
        let width = (ctx.screen_rect().width() - 24.0).clamp(260.0, 480.0);
        let height = (anchor.top() - ctx.screen_rect().top() - 120.0).clamp(140.0, 440.0);
        let popup = egui::Popup::new(egui::Id::new("composer-picker"), ctx.clone(), anchor, egui::LayerId::background())
        .open_bool(&mut open)
        .align(egui::RectAlign::TOP_END).gap(6.0).width(width)
        .close_behavior(if just_opened { egui::PopupCloseBehavior::IgnoreClicks } else { egui::PopupCloseBehavior::CloseOnClickOutside })
        .frame(egui::Frame::popup(&ctx.style()).inner_margin(12).corner_radius(14))
        .show(|ui| {
            ui.set_width(width - 24.0);
            ui.horizontal(|ui| {
                ui.strong(if mode == Mode::Emoji { "Emoji" } else { "GIFs" });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("×").clicked() { ui.close(); }
                });
            });
            ui.horizontal(|ui| {
                let search_width=if mode==Mode::Gif{ui.fonts(|f|f.layout_no_wrap("Search".into(),egui::TextStyle::Button.resolve(ui.style()),ui.visuals().text_color()).size().x)+ui.spacing().button_padding.x*2.0+ui.spacing().item_spacing.x}else{0.0};
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text(if mode == Mode::Emoji {
                            "Search emoji"
                        } else {
                            "Search GIFs"
                        })
                        .margin(Vec2::new(6.0,4.0))
                        .desired_width((width-24.0-12.0-search_width).max(100.0)),
                );
                if just_opened { r.request_focus(); }
                if mode == Mode::Gif
                    && (ui.button("Search").clicked()
                        || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))))
                    && !self.query.trim().is_empty()
                {
                    self.requested = self.query.clone();
                    self.pending = true;
                    command = Some(Command::Gifs(self.query.clone()));
                }
            });
            egui::ScrollArea::vertical()
                .id_salt("composer-picker-results")
                .max_height(height)
                .min_scrolled_height(height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if mode == Mode::Emoji {
                        let query = self.query.to_lowercase();
                        ui.horizontal_wrapped(|ui| {
                            for (emoji, name) in EMOJIS
                                .iter()
                                .filter(|(e, n)| n.contains(&query) || e.contains(&query))
                            {
                                if image_button(
                                    ui,
                                    images,
                                    Some(unicode_url(emoji)),
                                    emoji,
                                    Vec2::splat(32.0),
                                    false,
                                )
                                .on_hover_text(*name)
                                .clicked()
                                {
                                    picked = Some((*emoji).to_owned());
                                }
                            }
                        });
                        if !self.emojis.is_empty() {
                            ui.separator();
                            ui.label("This server");
                            ui.horizontal_wrapped(|ui| {
                                for emoji in self
                                    .emojis
                                    .iter()
                                    .filter(|e| {
                                        e.available != Some(false)
                                            && e.name.to_lowercase().contains(&query)
                                    })
                                    .take(512)
                                {
                                    if image_button(
                                        ui,
                                        images,
                                        emoji.image(),
                                        &emoji.name,
                                        Vec2::splat(32.0),
                                        false,
                                    )
                                    .on_hover_text(&emoji.name)
                                    .clicked()
                                    {
                                        picked = Some(emoji.token());
                                    }
                                }
                            });
                        }
                        // Nitro: every other server's emojis, a section per server.
                        let mut shown = 0;
                        for (server, emojis) in &self.other_servers {
                            let matching: Vec<_> = emojis.iter().filter(|e| e.available != Some(false) && e.name.to_lowercase().contains(&query)).take(300).collect();
                            if matching.is_empty() || shown >= 3000 { continue; }
                            shown += matching.len();
                            ui.separator();
                            ui.label(server);
                            ui.horizontal_wrapped(|ui| {
                                for emoji in matching {
                                    if image_button(ui, images, emoji.image(), &emoji.name, Vec2::splat(32.0), false)
                                        .on_hover_text(format!(":{}: · {server}", emoji.name))
                                        .clicked()
                                    {
                                        picked = Some(emoji.token());
                                    }
                                }
                            });
                        }
                    } else {
                        let tile_width = ((ui.available_width() - 8.0) / 2.0).floor();
                        let size = Vec2::new(tile_width, (tile_width * 0.56).round());
                        let browsing = self.requested.is_empty() && self.gifs.is_empty() && !self.pending;
                        if !browsing {
                            ui.horizontal(|ui| {
                                if ui.small_button("← Categories").clicked() {
                                    self.query.clear();
                                    self.requested.clear();
                                    self.gifs.clear();
                                    self.pending = false;
                                }
                                ui.strong(if self.requested == TRENDING { "Trending GIFs" } else { self.requested.as_str() });
                            });
                            ui.add_space(4.0);
                        }
                        if self.pending {
                            ui.spinner();
                        } else if browsing {
                            // Discord's GIF home: category tiles with moving previews.
                            if self.categories.is_empty() {
                                if self.categories_pending { ui.spinner(); }
                                else { ui.add(egui::Label::new("Search, then choose a GIF to add it to your message.").wrap()); }
                            }
                            let mut chosen = None;
                            egui::Grid::new("gif-categories").spacing(Vec2::splat(8.0)).show(ui, |ui| {
                                for (index, (label, preview)) in self.categories.iter().enumerate() {
                                    let shown = if label == TRENDING { "Trending GIFs" } else { label.as_str() };
                                    if gif_tile(ui, images, Some(preview), Some(shown), size).clicked() { chosen = Some(label.clone()); }
                                    if index % 2 == 1 { ui.end_row(); }
                                }
                            });
                            if let Some(label) = chosen {
                                self.query = if label == TRENDING { String::new() } else { label.clone() };
                                self.requested = label.clone();
                                self.pending = true;
                                command = Some(Command::Gifs(label));
                            }
                        } else if self.gifs.is_empty() {
                            ui.label("No GIFs found.");
                        }
                        if !browsing {
                            egui::Grid::new("gif-results").spacing(Vec2::splat(8.0)).show(ui, |ui| {
                                for (index, gif) in self.gifs.iter().take(40).enumerate() {
                                    if gif_tile(ui, images, Some(gif.image()), None, size).on_hover_text(&gif.title).clicked()
                                        && crate::model::safe_link(gif.share())
                                    {
                                        picked = Some(gif.share().to_owned());
                                    }
                                    if index % 2 == 1 { ui.end_row(); }
                                }
                            });
                        }
                        ui.add_space(6.0);
                    }
                });
        });
        self.popup_rect = popup.map(|p| p.response.rect);
        if !open || picked.is_some() {
            self.close();
        }
        (picked.map(|s| (self.channel.clone(), s)), command)
    }
}
/// Label of the Trending GIFs tile (also the request that asks for trending GIFs).
pub const TRENDING: &str = "Trending GIFs\u{1}";
/// The GIF home from Discord's /gifs/trending answer: Trending GIFs first (previewed by the top
/// trending GIF), then each category with its preview picture.
pub fn categories(home: &serde_json::Value) -> Vec<(String, String)> {
    let mut tiles = Vec::new();
    if let Some(top) = home["gifs"].as_array().and_then(|gifs| gifs.first()).and_then(|gif| gif["src"].as_str().or(gif["gif_src"].as_str())) {
        tiles.push((TRENDING.to_owned(), top.to_owned()));
    }
    for category in home["categories"].as_array().into_iter().flatten().take(40) {
        if let (Some(name), Some(src)) = (category["name"].as_str(), category["src"].as_str()) {
            if !name.is_empty() && name.len() <= 64 { tiles.push((name.to_owned(), src.to_owned())); }
        }
    }
    tiles
}
/// A GIF tile filling its cell: the picture cropped to fit, with a darkened, bold label for
/// category tiles.
fn gif_tile(ui: &mut egui::Ui, images: &mut Images, url: Option<&str>, label: Option<&str>, size: Vec2) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    if !ui.is_rect_visible(rect) { return response; }
    ui.painter().rect_filled(rect, 8, egui::Color32::from_gray(40));
    if let Some(url) = url.filter(|u| crate::assets::public_url(u)) {
        if let Some(texture) = images.texture_sized(url, rect.size(), ui.ctx()) {
            let image = images.dimensions(url, rect.size(), ui.ctx()).unwrap_or(rect.size());
            egui::Image::new((texture, rect.size())).uv(crate::identity::cover_uv(image, rect.size())).corner_radius(8).paint_at(ui, rect);
        }
    }
    if let Some(label) = label {
        ui.painter().rect_filled(rect, 8, egui::Color32::from_black_alpha(if response.hovered() { 90 } else { 140 }));
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(16.0), egui::Color32::WHITE);
    } else if response.hovered() {
        ui.painter().rect_stroke(rect, 8, egui::Stroke::new(2.0_f32, egui::Color32::from_gray(200)), egui::StrokeKind::Inside);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
fn image_button(
    ui: &mut egui::Ui,
    images: &mut Images,
    url: Option<String>,
    fallback: &str,
    size: Vec2,
    gif: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, 12, egui::Color32::from_gray(47));
    }
    if ui.is_rect_visible(rect) {
        // GIF results keep playing; animated emoji play only on hover.
        let texture = url.and_then(|url| if gif { images.texture_sized(&url, rect.size(), ui.ctx()) } else { images.texture_hover(&url, rect, ui.ctx()) });
        if let Some(texture) = texture {
            egui::Image::new((texture, size - Vec2::splat(6.0))).paint_at(ui, rect.shrink(3.0));
        } else {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                fallback.chars().take(3).collect::<String>(),
                egui::FontId::proportional(19.0),
                egui::Color32::WHITE,
            );
        }
    }
    response
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gif_home_lists_trending_first_then_categories() {
        let home = serde_json::json!({"categories":[{"name":"hello","src":"https://static.klipy.com/hello.gif"},{"name":"","src":"x"}],"gifs":[{"src":"https://static.klipy.com/top.gif"}]});
        assert_eq!(categories(&home), vec![(TRENDING.to_owned(),"https://static.klipy.com/top.gif".to_owned()),("hello".to_owned(),"https://static.klipy.com/hello.gif".to_owned())]);
        assert!(categories(&serde_json::json!({})).is_empty());
    }
    fn input(events:Vec<egui::Event>)->egui::RawInput{egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,Vec2::new(720.0,453.0))),events,..Default::default()}}
    #[test]
    fn pickers_follow_their_anchor_stay_on_screen_and_dismiss(){
        for mode in [Mode::Emoji,Mode::Gif]{
            let ctx=egui::Context::default();let mut images=Images::new(&ctx);let mut picker=Picker::default();
            let anchor=egui::Rect::from_min_size(egui::pos2(650.0,390.0),Vec2::splat(30.0));
            picker.open(mode,"chat",None,anchor);picker.query="no matching emoji".into();
            for _ in 0..3{let _=ctx.run(input(vec![]),|ctx|{picker.show(ctx,&mut images);});}
            let rect=picker.popup_rect.unwrap();assert!(rect.left()>=0.0&&rect.top()>=0.0&&rect.right()<=720.0&&rect.bottom()<=453.0,"{rect:?}");assert!(rect.bottom()<=anchor.top(),"must open above the composer: {rect:?}");
            let anchor=anchor.translate(egui::vec2(-100.0,-50.0));picker.anchor=Some(anchor);
            for _ in 0..3{let _=ctx.run(input(vec![]),|ctx|{picker.show(ctx,&mut images);});}
            let moved=picker.popup_rect.unwrap();assert!((moved.right()-rect.right()+100.0).abs()<2.0,"{rect:?} -> {moved:?}");
            let events=vec![egui::Event::PointerMoved(egui::pos2(10.0,440.0)),egui::Event::PointerButton{pos:egui::pos2(10.0,440.0),button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()},egui::Event::PointerButton{pos:egui::pos2(10.0,440.0),button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}];
            let _=ctx.run(input(events),|ctx|{picker.show(ctx,&mut images);});assert!(picker.mode.is_none());
            picker.open(mode,"chat",None,anchor);
            let _=ctx.run(input(vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]),|ctx|{picker.show(ctx,&mut images);});assert!(picker.mode.is_none());
            picker.open(mode,"chat",None,anchor);picker.open(mode,"chat",None,anchor);assert!(picker.mode.is_none(),"same button toggles closed");
        }
    }
    #[test]
    fn custom_emoji_tokens_and_urls() {
        let e = CustomEmoji {
            id: "123".into(),
            name: "dance".into(),
            animated: true,
            ..Default::default()
        };
        assert_eq!(e.token(), "<a:dance:123>");
        assert!(e.image().unwrap().contains("123.gif"));
        assert!(emoji_url("../token", false).is_none());
        assert!(unicode_url("❤️").ends_with("2764.png"));
    }
}
