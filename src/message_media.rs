use crate::{assets::Images, media_picker, model::{Message,Embed}};
use eframe::egui::{self, Vec2};
fn link_token(token:&str)->&str{token.trim_matches(['<','>']).trim_end_matches([',',';'])}
pub fn direct_images(text:&str)->Vec<String>{
    text.split_whitespace().map(link_token).filter(|s|crate::assets::public_url(s)).take(20).map(str::to_owned).collect()
}
pub fn media_embed(embed:&Embed)->bool{
    if !matches!(embed.kind.as_str(),"image"|"gifv") && (embed.title.is_some()||embed.description.is_some()){return false;}
    [embed.image.as_ref(),embed.thumbnail.as_ref(),embed.video.as_ref().filter(|_|embed.kind=="gifv")].into_iter().flatten().any(|image|image.url.as_deref().is_some_and(crate::assets::public_url)||image.proxy_url.as_deref().is_some_and(crate::assets::public_url))
}
pub fn visible_content(message:&Message,images:bool)->String{
    if !images{return message.content.clone();}
    let direct=if message.embeds.is_empty(){direct_images(&message.content)}else{vec![]};
    let embeds:Vec<_>=message.embeds.iter().filter(|e|media_embed(e)).filter_map(|e|e.url.as_deref()).collect();
    let mut result=String::new();let mut start=0;
    for (index,c) in message.content.char_indices(){if c.is_whitespace(){let word=&message.content[start..index];let link=link_token(word);if !direct.iter().any(|s|s==link)&&!embeds.contains(&link){result.push_str(word);}result.push(c);start=index+c.len_utf8();}}
    let word=&message.content[start..];let link=link_token(word);if !direct.iter().any(|s|s==link)&&!embeds.contains(&link){result.push_str(word);}
    result.trim().to_owned()
}
#[derive(Debug, PartialEq)]
pub enum Span<'a> {
    Text(&'a str),
    Emoji { label: &'a str, url: String },
}
pub fn spans(text: &str) -> Vec<Span<'_>> {
    let mut result = Vec::new();
    let mut offset = 0;
    let mut plain = 0;
    while offset < text.len() {
        let rest = &text[offset..];
        let mut matched = None;
        if rest.starts_with("<:") || rest.starts_with("<a:") {
            if let Some(end) = rest.find('>').filter(|i| *i < 100) {
                let token = &rest[..=end];
                let pieces: Vec<_> = token[1..end].split(':').collect();
                if pieces.len() == 3 && !pieces[1].is_empty() {
                    if let Some(url) = media_picker::emoji_url(pieces[2], pieces[0] == "a") {
                        matched = Some((token.len(), url));
                    }
                }
            }
        }
        if matched.is_none() {
            for (emoji, _) in media_picker::EMOJIS {
                if rest.starts_with(emoji) {
                    matched = Some((emoji.len(), media_picker::unicode_url(emoji)));
                    break;
                }
            }
        }
        if let Some((length, url)) = matched {
            if plain < offset {
                result.push(Span::Text(&text[plain..offset]));
            }
            result.push(Span::Emoji {
                label: &text[offset..offset + length],
                url,
            });
            offset += length;
            plain = offset;
        } else {
            offset += rest.chars().next().unwrap().len_utf8();
        }
    }
    if plain < text.len() {
        result.push(Span::Text(&text[plain..]));
    }
    result
}
pub fn body(ui: &mut egui::Ui, images: &mut Images, text: &str) ->Vec<egui::Response> {
    let spans = spans(text);
    if !spans.iter().any(|s| matches!(s, Span::Emoji { .. })) {
        return vec![ui.add(egui::Label::new(text).wrap().selectable(true))];
    }
    let mut responses=vec![];
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(2.0, 3.0);
        for span in spans {
            match span {
                Span::Text(text) => {
                    for line in text.split_inclusive('\n') {
                        for word in line.split_inclusive(' ') {
                            if !word.trim_end_matches('\n').is_empty() {
                                responses.push(ui.add(
                                    egui::Label::new(word.trim_end_matches('\n')).selectable(true),
                                ));
                            }
                        }
                        if line.ends_with('\n') {
                            ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), 0.0),
                                egui::Sense::hover(),
                            );
                        }
                    }
                }
                Span::Emoji { label, url } => {
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::splat(23.0), egui::Sense::click());
                    if ui.is_rect_visible(rect) {
                        if let Some(texture) = images.texture(&url, ui.ctx()) {
                            egui::Image::new((texture, rect.size())).paint_at(ui, rect);
                        } else {
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                label,
                                egui::FontId::proportional(18.0),
                                egui::Color32::WHITE,
                            );
                        }
                    }
                    responses.push(response.on_hover_text(label));
                }
            }
        }
    });
    responses
}
pub fn picture(
    ui: &mut egui::Ui,
    images: &mut Images,
    url: &str,
    width: Option<u32>,
    height: Option<u32>,
) {
    if !crate::assets::public_url(url) {
        return;
    }
    let max = ui.available_width().min(360.0);
    let aspect = width
        .zip(height)
        .filter(|(w, h)| *w > 0 && *h > 0)
        .map(|(w, h)| h as f32 / w as f32)
        .or_else(||images.dimensions(url,Vec2::splat(max),ui.ctx()).map(|size|size.y/size.x.max(1.)))
        .unwrap_or(0.75)
        .clamp(0.3, 1.5);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(max, max * aspect), egui::Sense::click());
    ui.painter()
        .rect_filled(rect, 19, egui::Color32::from_gray(36));
    if ui.is_rect_visible(rect) {
        if let Some(texture) = images.texture_sized(url,rect.size(), ui.ctx()) {
            egui::Image::new((texture, rect.size()))
                .uv(crate::identity::cover_uv(images.dimensions(url,rect.size(),ui.ctx()).unwrap_or(rect.size()),rect.size()))
                .corner_radius(19)
                .paint_at(ui, rect);
        } else {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                if images.failed(url){"Image unavailable · right-click for options"}else{"Loading image…"},
                egui::FontId::proportional(12.0),
                egui::Color32::GRAY,
            );
        }
    }
    response.context_menu(|ui|{if ui.button("Open image").clicked(){ui.ctx().open_url(egui::OpenUrl::new_tab(url));ui.close();}if ui.button("Copy image link").clicked(){ui.ctx().copy_text(url.to_owned());ui.close();}});
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn hides_only_rendered_media_links_and_keeps_surrounding_words(){
        let mut m=Message{content:"look https://cdn.discordapp.com/attachments/1/cat.png wow\nhttps://example.com/info".into(),..Default::default()};
        assert_eq!(direct_images(&m.content).len(),1);
        assert_eq!(visible_content(&m,true),"look  wow\nhttps://example.com/info");
        assert_eq!(visible_content(&m,false),m.content);
        m.content="a GIF https://tenor.com/view/cat-123 nice".into();
        m.embeds.push(Embed{kind:"gifv".into(),url:Some("https://tenor.com/view/cat-123".into()),video:Some(crate::model::EmbedImage{url:Some("https://media.tenor.com/cat.mp4".into()),..Default::default()}),..Default::default()});
        assert_eq!(visible_content(&m,true),"a GIF  nice");
        m.embeds[0].video=None;assert_eq!(visible_content(&m,true),m.content);
    }
    #[test]
    fn right_click_opens_a_menu_on_selectable_message_text(){
        let ctx=egui::Context::default();let mut images=Images::new(&ctx);let mut rect=egui::Rect::NOTHING;let mut opened=false;
        for phase in 0..4 {
            let mut input=egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,Vec2::new(400.,150.))),..Default::default()};
            if phase>0{let pos=rect.center();input.events.push(egui::Event::PointerMoved(pos));if phase<3{input.events.push(egui::Event::PointerButton{pos,button:egui::PointerButton::Secondary,pressed:phase==1,modifiers:Default::default()});}}
            let _=ctx.run(input,|ctx|{egui::CentralPanel::default().show(ctx,|ui|{for response in body(ui,&mut images,"A message that can be selected and right-clicked"){rect=response.rect;response.context_menu(|ui|{opened=true;ui.label("Reply");});}});});
        }
        assert!(opened,"Selectable message text swallowed its context menu");
    }
    #[test]
    fn unicode_and_custom_keep_surrounding_text() {
        let result = spans("hi 😀 <a:dance:123> bye");
        assert_eq!(result.len(), 5);
        assert!(matches!(&result[3],Span::Emoji{url,..}if url.contains("123.gif")));
        assert_eq!(spans("literal <bad>"), vec![Span::Text("literal <bad>")]);
        assert_eq!(
            spans("<:evil:../secret>"),
            vec![Span::Text("<:evil:../secret>")]
        );
    }
}
