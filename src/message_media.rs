use crate::{assets::Images, media_picker, model::{Message,Embed,EmbedImage}};
use eframe::egui::{self, Vec2};
/// Discord's link blue, so links stand out from message text in every theme.
pub const LINK: egui::Color32 = egui::Color32::from_rgb(0, 168, 252);
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
/// Turns <@id>, <@!id>, <@&role> and <#channel> into readable names. `resolve` gets the kind
/// ('@' user, '&' role, '#' channel) and the id; unknown ids read like Discord's.
pub fn replace_mentions(text:&str,resolve:impl Fn(char,&str)->Option<String>)->String{
    let mut out=String::with_capacity(text.len());let mut rest=text;
    while let Some(start)=rest.find('<'){
        out.push_str(&rest[..start]);let tail=&rest[start..];
        let (kind,skip)=if tail.starts_with("<@!"){('@',3)}else if tail.starts_with("<@&"){('&',3)}else if tail.starts_with("<@"){('@',2)}else if tail.starts_with("<#"){('#',2)}else{('?',0)};
        let digits=tail.get(skip..).map(|t|t.chars().take_while(char::is_ascii_digit).count()).unwrap_or(0);
        if kind!='?'&&(1..=20).contains(&digits)&&tail[skip+digits..].starts_with('>'){
            let id=&tail[skip..skip+digits];
            out.push_str(&resolve(kind,id).unwrap_or_else(||match kind{'#'=>"#unknown".into(),'&'=>"@unknown-role".into(),_=>"@unknown-user".into()}));
            rest=&tail[skip+digits+1..];
        }else{out.push('<');rest=&tail[1..];}
    }
    out.push_str(rest);out
}
/// The @word being typed just before the cursor (a char index): its start and the text after @.
pub fn mention_token(text:&str,cursor:usize)->Option<(usize,String)>{
    let chars:Vec<char>=text.chars().collect();let cursor=cursor.min(chars.len());
    let start=chars[..cursor].iter().rposition(|c|c.is_whitespace()).map(|i|i+1).unwrap_or(0);
    let word:String=chars[start..cursor].iter().collect();
    let query=word.strip_prefix('@')?;
    (query.chars().count()<=32&&!query.contains('@')).then(||(start,query.to_owned()))
}
/// Swaps each picked @username for <@id> before sending, only where the whole name was typed.
pub fn apply_mentions(text:&str,ids:&std::collections::HashMap<String,String>)->String{
    let mut labels:Vec<_>=ids.iter().collect();labels.sort_by_key(|(label,_)|std::cmp::Reverse(label.len()));
    let mut out=text.to_owned();
    for (label,id) in labels{
        let mut result=String::new();let mut rest=out.as_str();
        while let Some(at)=rest.find(label.as_str()){
            let after=rest[at+label.len()..].chars().next();
            let before=if at>0{rest[..at].chars().last()}else{result.chars().last()};
            let whole=!after.is_some_and(|c|c.is_alphanumeric()||c=='_'||c=='.')&&!before.is_some_and(|c|c.is_alphanumeric());
            result.push_str(&rest[..at]);
            if whole{result.push_str(&format!("<@{id}>"));}else{result.push_str(label);}
            rest=&rest[at+label.len()..];
        }
        result.push_str(rest);out=result;
    }
    out
}
/// The web address inside a word, without surrounding <> or trailing punctuation.
pub fn link_in(word:&str)->Option<&str>{
    let link=word.trim_start_matches(['<','(']).trim_end_matches(['>',',',';','.','!','?',':',')','\'','"']);
    ((link.starts_with("https://")||link.starts_with("http://"))&&crate::model::safe_link(link)).then_some(link)
}
pub fn body(ui: &mut egui::Ui, images: &mut Images, text: &str) ->Vec<egui::Response> {
    let spans = spans(text);
    if !spans.iter().any(|s| matches!(s, Span::Emoji { .. })) && !text.split_whitespace().any(|w|link_in(w).is_some()) {
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
                            let word=word.trim_end_matches('\n');
                            if let Some(link)=link_in(word.trim_end()) {
                                // Links open in the browser; trailing punctuation stays plain text.
                                let start=word.find(link).unwrap_or(0);
                                let rest=word[start+link.len()..].trim_start_matches(['>']);
                                let shown=if word[..start].ends_with('<'){link}else{&word[..start+link.len()]};
                                let response=ui.add(egui::Hyperlink::from_label_and_url(egui::RichText::new(shown.trim_start_matches(['<'])).color(LINK),link));
                                response.context_menu(|ui|{if ui.button("Copy link").clicked(){ui.ctx().copy_text(link.to_owned());ui.close();}});
                                if !rest.is_empty(){responses.push(ui.add(egui::Label::new(rest).selectable(true)));}
                            } else if !word.is_empty() {
                                responses.push(ui.add(
                                    egui::Label::new(word).selectable(true),
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
                        if let Some(texture) = images.texture(&url, rect, ui.ctx()) {
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
    gif: bool,
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
        // Chat GIFs keep playing; other animated pictures play only on hover.
        let texture = if gif { images.texture_sized(url, rect.size(), ui.ctx()) } else { images.texture_hover(url, rect, ui.ctx()) };
        if let Some(texture) = texture {
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
    if response.clicked(){open_viewer(ui.ctx(),url);}
    let response=response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.context_menu(|ui|{if ui.button("Open image").clicked(){ui.ctx().open_url(egui::OpenUrl::new_tab(url));ui.close();}if ui.button("Copy image link").clicked(){ui.ctx().copy_text(url.to_owned());ui.close();}if ui.button("Save image…").clicked(){save_image(ui.ctx(),url);ui.close();}});
}
/// A file name for a picture's address: its last path segment, made safe for Windows.
fn file_name(url:&str)->String{
    let path=url.split(['?','#']).next().unwrap_or("");
    let name:String=path.rsplit('/').next().unwrap_or("").chars().map(|c|if c.is_control()||r#"<>:"/\|?*"#.contains(c){'_'}else{c}).take(120).collect();
    let name=name.trim_matches(['.',' ']).to_owned();
    if name.is_empty(){"image.png".into()}else if name.contains('.'){name}else{format!("{name}.png")}
}
static SAVE_STATUS:std::sync::Mutex<Option<(String,std::time::Instant)>>=std::sync::Mutex::new(None);
fn report(ctx:&egui::Context,text:String){if let Ok(mut status)=SAVE_STATUS.lock(){*status=Some((text,std::time::Instant::now()));}ctx.request_repaint();}
/// Asks where to save a picture, then downloads it there in the background.
pub fn save_image(ctx:&egui::Context,url:&str){
    if !crate::assets::public_url(url){return;}
    let (ctx,url)=(ctx.clone(),url.to_owned());
    std::thread::spawn(move||{
        let name=file_name(&url);
        let extension=name.rsplit('.').next().unwrap_or("png").to_ascii_lowercase();
        let Some(path)=rfd::FileDialog::new().set_file_name(&name).add_filter("Image",&[extension.as_str()]).save_file() else{return};
        report(&ctx,"Saving image…".into());
        let result=(||->Result<(),String>{
            use std::io::Read;
            let client=reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(60)).build().map_err(|_|"Could not start the download.".to_owned())?;
            let response=client.get(&url).send().map_err(|_|"Could not reach the image.".to_owned())?;
            if !response.status().is_success(){return Err(format!("The image could not be downloaded ({}).",response.status().as_u16()));}
            // Pictures over 100 MB are refused rather than filling memory.
            let mut bytes=vec![];response.take(100*1024*1024+1).read_to_end(&mut bytes).map_err(|_|"The download was interrupted.".to_owned())?;
            if bytes.len()>100*1024*1024{return Err("The image is larger than 100 MB.".into());}
            std::fs::write(&path,bytes).map_err(|_|"Could not write the file there.".to_owned())
        })();
        report(&ctx,match result{Ok(())=>format!("Saved {}",path.file_name().map(|n|n.to_string_lossy().into_owned()).unwrap_or_default()),Err(error)=>error});
    });
}
/// A short note at the bottom of the window after saving a picture.
pub fn save_status(ctx:&egui::Context){
    let Some((text,at))=SAVE_STATUS.lock().ok().and_then(|s|s.clone()) else{return};
    let age=at.elapsed().as_secs_f32();
    if age>3.5&&text!="Saving image…"{if let Ok(mut status)=SAVE_STATUS.lock(){*status=None;}return;}
    egui::Area::new(egui::Id::new("image-save-status")).order(egui::Order::Tooltip).anchor(egui::Align2::CENTER_BOTTOM,Vec2::new(0.0,-24.0)).interactable(false).show(ctx,|ui|{
        egui::Frame::NONE.fill(egui::Color32::from_gray(32)).stroke(egui::Stroke::new(1.0_f32,egui::Color32::from_gray(60))).corner_radius(8).inner_margin(egui::Margin::symmetric(14,8)).show(ui,|ui|{ui.label(egui::RichText::new(&text).color(egui::Color32::WHITE));});
    });
    ctx.request_repaint_after(std::time::Duration::from_millis(250));
}
fn viewer_id()->egui::Id{egui::Id::new("eclipse-image-viewer")}
/// What the picture viewer shows: the image and who sent it.
#[derive(Clone,Default)]
pub struct Viewer{url:String,name:String,time:String,avatar:Option<String>,captioned:bool,zoom:bool}
/// Opens the picture viewer for a chat picture.
pub fn open_viewer(ctx:&egui::Context,url:&str){ctx.data_mut(|d|d.insert_temp(viewer_id(),Viewer{url:url.to_owned(),..Default::default()}));}
/// Whether a viewer was just opened and still needs its sender and time.
pub fn viewer_needs_caption(ctx:&egui::Context)->bool{ctx.data(|d|d.get_temp::<Viewer>(viewer_id())).is_some_and(|v|!v.captioned)}
/// Called after each message is drawn: a viewer opened from that message gets its sender and time.
pub fn caption_viewer(ctx:&egui::Context,name:&str,time:&str,avatar:Option<String>){
    let Some(mut viewer)=ctx.data(|d|d.get_temp::<Viewer>(viewer_id())) else{return};
    if viewer.captioned{return;}
    viewer.name=name.to_owned();viewer.time=time.to_owned();viewer.avatar=avatar;viewer.captioned=true;
    ctx.data_mut(|d|d.insert_temp(viewer_id(),viewer));
}
/// One round toolbar button drawn with the painter, like Discord's media viewer.
fn tool(ui:&mut egui::Ui,rect:egui::Rect,tip:&str,draw:impl Fn(&egui::Painter,egui::Pos2,egui::Stroke))->egui::Response{
    let response=ui.interact(rect,ui.id().with(tip),egui::Sense::click()).on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.hovered(){ui.painter().rect_filled(rect,6,egui::Color32::from_white_alpha(18));}
    draw(ui.painter(),rect.center(),egui::Stroke::new(1.6_f32,egui::Color32::from_gray(225)));
    response
}
/// The picture viewer, like Discord's: the picture at its own size (shrunk only to fit) over the
/// dimmed app, the sender and time at the top left, and zoom, open, copy and close at the top right.
/// Click outside the picture or press Esc to close it.
pub fn viewer(ctx:&egui::Context,images:&mut Images){
    let Some(mut viewer)=ctx.data(|d|d.get_temp::<Viewer>(viewer_id())) else{return};
    let url=viewer.url.clone();
    let mut close=ctx.input(|i|i.key_pressed(egui::Key::Escape));
    let screen=ctx.screen_rect();
    egui::Area::new(egui::Id::new("image-viewer-area")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx,|ui|{
        use egui::{Color32,pos2};
        let (rect,backdrop)=ui.allocate_exact_size(screen.size(),egui::Sense::click());
        ui.painter().rect_filled(rect,0,Color32::from_black_alpha(205));
        // The picture, never stretched past its own size unless zoomed.
        let bounds=egui::Rect::from_min_max(rect.min+Vec2::new(40.0,80.0),rect.max-Vec2::new(40.0,40.0));
        let request=Vec2::splat(1024.0/ctx.pixels_per_point());
        let mut picture=egui::Rect::from_center_size(bounds.center(),Vec2::ZERO);
        if let Some(texture)=images.texture_sized(&url,request,ctx){
            let size=images.dimensions(&url,request,ctx).unwrap_or(bounds.size())/ctx.pixels_per_point();
            let fit=(bounds.width()/size.x.max(1.0)).min(bounds.height()/size.y.max(1.0)).min(1.0);
            let scale=if viewer.zoom{(fit*2.0).max(1.0)}else{fit};
            picture=egui::Rect::from_center_size(bounds.center(),size*scale);
            ui.painter().with_clip_rect(bounds.expand(20.0)).add(egui::Shape::image(texture,picture,egui::Rect::from_min_max(pos2(0.0,0.0),pos2(1.0,1.0)),Color32::WHITE));
        }else{
            let failed=images.failed(&url);
            ui.painter().text(bounds.center(),egui::Align2::CENTER_CENTER,if failed{"Image unavailable"}else{"Loading image…"},egui::FontId::proportional(14.0),Color32::GRAY);
            if !failed{ctx.request_repaint();}
        }
        // Sender and time, top left.
        if viewer.captioned{
            let avatar=egui::Rect::from_min_size(rect.min+Vec2::new(16.0,14.0),Vec2::splat(40.0));
            ui.painter().circle_filled(avatar.center(),20.0,Color32::from_gray(60));
            if let Some(texture)=viewer.avatar.as_deref().and_then(|a|images.texture(a,avatar,ctx)){egui::Image::new((texture,avatar.size())).corner_radius(20).paint_at(ui,avatar);}
            ui.painter().text(avatar.right_top()+Vec2::new(12.0,2.0),egui::Align2::LEFT_TOP,&viewer.name,egui::FontId::proportional(16.0),Color32::WHITE);
            ui.painter().text(avatar.right_bottom()+Vec2::new(12.0,-2.0),egui::Align2::LEFT_BOTTOM,&viewer.time,egui::FontId::proportional(12.0),Color32::from_gray(180));
        }
        // Toolbar and close button, top right.
        let close_rect=egui::Rect::from_min_size(pos2(rect.right()-56.0,rect.top()+16.0),Vec2::splat(36.0));
        ui.painter().rect_filled(close_rect,8,Color32::from_gray(28));ui.painter().rect_stroke(close_rect,8,egui::Stroke::new(1.0_f32,Color32::from_gray(60)),egui::StrokeKind::Inside);
        if tool(ui,close_rect,"Close",|p,c,s|{let d=6.0;p.line_segment([c-Vec2::splat(d),c+Vec2::splat(d)],s);p.line_segment([c+Vec2::new(-d,d),c+Vec2::new(d,-d)],s);}).clicked(){close=true;}
        let bar=egui::Rect::from_min_size(pos2(close_rect.left()-12.0-4.0*36.0-8.0,close_rect.top()),Vec2::new(4.0*36.0+8.0,36.0));
        ui.painter().rect_filled(bar,8,Color32::from_gray(28));ui.painter().rect_stroke(bar,8,egui::Stroke::new(1.0_f32,Color32::from_gray(60)),egui::StrokeKind::Inside);
        let slot=|i:f32|egui::Rect::from_min_size(bar.min+Vec2::new(4.0+i*36.0,0.0),Vec2::splat(36.0));
        let zoom=viewer.zoom;
        if tool(ui,slot(0.0),if zoom{"Zoom out"}else{"Zoom in"},|p,c,s|{let o=c-Vec2::splat(2.0);p.circle_stroke(o,6.0,s);p.line_segment([o+Vec2::splat(4.5),o+Vec2::splat(9.0)],s);p.line_segment([o-Vec2::new(3.0,0.0),o+Vec2::new(3.0,0.0)],s);if !zoom{p.line_segment([o-Vec2::new(0.0,3.0),o+Vec2::new(0.0,3.0)],s);}}).clicked(){viewer.zoom=!viewer.zoom;}
        if tool(ui,slot(1.0),"Open in browser",|p,c,s|{let r=egui::Rect::from_center_size(c+Vec2::new(-1.0,1.0),Vec2::splat(12.0));p.rect_stroke(r,2,s,egui::StrokeKind::Middle);p.line_segment([c,c+Vec2::new(7.0,-7.0)],s);p.line_segment([c+Vec2::new(2.0,-7.0),c+Vec2::new(7.0,-7.0)],s);p.line_segment([c+Vec2::new(7.0,-7.0),c+Vec2::new(7.0,-2.0)],s);}).clicked(){ctx.open_url(egui::OpenUrl::new_tab(&url));}
        if tool(ui,slot(3.0),"Save image",|p,c,s|{p.line_segment([c-Vec2::new(0.0,7.0),c+Vec2::new(0.0,3.0)],s);p.line_segment([c+Vec2::new(-4.0,-1.0),c+Vec2::new(0.0,3.0)],s);p.line_segment([c+Vec2::new(4.0,-1.0),c+Vec2::new(0.0,3.0)],s);p.line_segment([c+Vec2::new(-7.0,7.0),c+Vec2::new(7.0,7.0)],s);}).clicked(){save_image(ctx,&url);}
        if tool(ui,slot(2.0),"Copy link",|p,c,s|{p.rect_stroke(egui::Rect::from_center_size(c+Vec2::splat(2.0),Vec2::splat(10.0)),2,s,egui::StrokeKind::Middle);p.rect_stroke(egui::Rect::from_center_size(c-Vec2::splat(2.0),Vec2::splat(10.0)),2,s,egui::StrokeKind::Middle);}).clicked(){ctx.copy_text(url.clone());}
        if backdrop.clicked()&&!backdrop.interact_pointer_pos().is_some_and(|p|picture.contains(p)){close=true;}
    });
    ctx.data_mut(|d|if close{d.remove::<Viewer>(viewer_id());}else{d.insert_temp(viewer_id(),viewer);});
}
/// A Discord-style link preview in one card: colored side bar, site name, linked title, description
/// and fields, with a small thumbnail beside the text or the large image inside the card.
pub fn embed_card(ui:&mut egui::Ui,images:&mut Images,embed:&Embed,show_images:bool){
    use egui::{Color32,RichText};
    let public=|image:&EmbedImage|image.url.as_deref().filter(|u|crate::assets::public_url(u)).or(image.proxy_url.as_deref().filter(|u|crate::assets::public_url(u))).map(|u|(u.to_owned(),image.width,image.height));
    let large=if !show_images{None}else{embed.image.as_ref().and_then(public).or_else(||matches!(embed.kind.as_str(),"video"|"gifv").then(||embed.thumbnail.as_ref().and_then(public)).flatten())};
    let small=if !show_images||large.is_some(){None}else{embed.thumbnail.as_ref().and_then(public)};
    let bar=embed.color.filter(|c|*c!=0).map(|c|Color32::from_rgb((c>>16)as u8,(c>>8)as u8,c as u8)).unwrap_or(Color32::from_gray(78));
    let muted=Color32::from_gray(170);
    let width=ui.available_width().min(440.0);
    let card=egui::Frame::NONE.fill(Color32::from_gray(36)).stroke(egui::Stroke::new(1.0_f32,Color32::from_gray(46))).corner_radius(6).inner_margin(egui::Margin{left:14,right:12,top:10,bottom:12}).show(ui,|ui|{
        ui.set_width(width-26.0);
        ui.horizontal_top(|ui|{
            let text_width=if small.is_some(){ui.available_width()-92.0}else{ui.available_width()};
            ui.vertical(|ui|{
                ui.set_width(text_width);ui.spacing_mut().item_spacing.y=4.0;
                let line=|ui:&mut egui::Ui,text:RichText,url:Option<&str>|match url.filter(|u|crate::model::safe_link(u)){
                    Some(url)=>{ui.add(egui::Hyperlink::from_label_and_url(text,url));},
                    None=>{ui.add(egui::Label::new(text).wrap());},
                };
                if let Some(site)=&embed.provider{if let Some(name)=&site.name{line(ui,RichText::new(name).size(12.0).color(muted),site.url.as_deref());}}
                if let Some(author)=&embed.author{if let Some(name)=&author.name{line(ui,RichText::new(name).size(13.0).strong().color(Color32::from_gray(237)),author.url.as_deref());}}
                if let Some(title)=&embed.title{line(ui,RichText::new(title).size(15.0).strong().color(if embed.url.is_some(){LINK}else{Color32::from_gray(237)}),embed.url.as_deref());}
                if let Some(description)=&embed.description{body(ui,images,&description.chars().take(1000).collect::<String>());}
                for field in embed.fields.iter().take(25){
                    ui.add_space(2.0);ui.label(RichText::new(&field.name).size(13.0).strong().color(Color32::from_gray(237)));
                    body(ui,images,&field.value.chars().take(1024).collect::<String>());
                }
            });
            if let Some((url,_,_))=&small{
                ui.add_space(12.0);
                let (rect,response)=ui.allocate_exact_size(Vec2::splat(80.0),egui::Sense::click());
                if ui.is_rect_visible(rect){
                    if let Some(texture)=images.texture_hover(url,rect,ui.ctx()){
                        egui::Image::new((texture,rect.size())).uv(crate::identity::cover_uv(images.dimensions(url,rect.size(),ui.ctx()).unwrap_or(rect.size()),rect.size())).corner_radius(6).paint_at(ui,rect);
                    }else{ui.painter().rect_filled(rect,6,Color32::from_gray(46));}
                }
                // Like other chat pictures: click for the larger view, right-click for open, copy and save.
                if response.clicked(){open_viewer(ui.ctx(),url);}
                response.on_hover_cursor(egui::CursorIcon::PointingHand).context_menu(|ui|{if ui.button("Open image").clicked(){ui.ctx().open_url(egui::OpenUrl::new_tab(url));ui.close();}if ui.button("Copy image link").clicked(){ui.ctx().copy_text(url.to_owned());ui.close();}if ui.button("Save image…").clicked(){save_image(ui.ctx(),url);ui.close();}});
            }
        });
        if let Some((url,w,h))=&large{ui.add_space(8.0);picture(ui,images,url,*w,*h,false);}
    }).response.rect;
    ui.painter().rect_filled(egui::Rect::from_min_max(card.left_top(),egui::pos2(card.left()+4.0,card.bottom())),egui::CornerRadius{nw:6,sw:6,ne:0,se:0},bar);
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
    fn links_are_found_without_brackets_or_trailing_punctuation(){
        assert_eq!(link_in("https://stremio-addons.net/addons/magnetflix"),Some("https://stremio-addons.net/addons/magnetflix"));
        assert_eq!(link_in("<https://example.com/a>,"),Some("https://example.com/a"));
        assert_eq!(link_in("(https://example.com)."),Some("https://example.com"));
        assert_eq!(link_in("javascript:alert(1)"),None);assert_eq!(link_in("example.com"),None);
    }
    #[test]
    fn clicking_a_picture_opens_the_viewer_and_escape_closes_it(){
        let ctx=egui::Context::default();let mut images=Images::new(&ctx);let mut rect=egui::Rect::NOTHING;
        let url="https://cdn.discordapp.com/attachments/1/2/cat.png";
        let open=|ctx:&egui::Context|ctx.data(|d|d.get_temp::<Viewer>(viewer_id())).map(|v|v.url);
        for phase in 0..4 {
            let mut input=egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,Vec2::new(500.,500.))),..Default::default()};
            match phase{1|2=>{let pos=rect.center();input.events.push(egui::Event::PointerMoved(pos));input.events.push(egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:phase==1,modifiers:Default::default()});},
                3=>input.events.push(egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}),_=>{}}
            let _=ctx.run(input,|ctx|{egui::CentralPanel::default().show(ctx,|ui|{let before=ui.cursor().min;picture(ui,&mut images,url,Some(300),Some(200),true);rect=egui::Rect::from_min_max(before,ui.min_rect().max);});viewer(ctx,&mut images);});
            if phase==2{assert_eq!(open(&ctx).as_deref(),Some(url),"click did not open the viewer");}
        }
        assert!(open(&ctx).is_none(),"Esc did not close the viewer");
    }
    #[test]
    fn saved_pictures_get_safe_file_names(){
        assert_eq!(file_name("https://cdn.discordapp.com/attachments/1/2/cat.png?ex=1&hm=2"),"cat.png");
        assert_eq!(file_name("https://media.discordapp.net/x/a%3Cb>.gif"),"a%3Cb_.gif");
        assert_eq!(file_name("https://example.com/"),"image.png");
        assert_eq!(file_name("https://example.com/photo"),"photo.png");
    }
    #[test]
    fn mentions_read_as_names_and_picked_names_are_sent_as_ids(){
        let resolve=|kind:char,id:&str|match (kind,id){('@',"1")=>Some("@husain".to_owned()),('&',"9")=>Some("@Mods".to_owned()),('#',"5")=>Some("#general".to_owned()),_=>None};
        assert_eq!(replace_mentions("hi <@1> and <@!1>, ask <@&9> in <#5>",resolve),"hi @husain and @husain, ask @Mods in #general");
        assert_eq!(replace_mentions("<@2> <@abc> a<b",resolve),"@unknown-user <@abc> a<b");
        assert_eq!(mention_token("hey @hu",7),Some((4,"hu".into())));
        assert_eq!(mention_token("@",1),Some((0,String::new())));
        assert_eq!(mention_token("mail a@b",8),None);assert_eq!(mention_token("hey hu",6),None);
        let ids:std::collections::HashMap<String,String>=[("@husain".to_owned(),"1".to_owned()),("@hus".to_owned(),"2".to_owned())].into();
        assert_eq!(apply_mentions("@husain and @hus, not @husainx or a@hus",&ids),"<@1> and <@2>, not @husainx or a@hus");
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
