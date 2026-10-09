//! Native rendering of Discord-supplied identity metadata; no entitlement fabrication.
use crate::{assets, model::User};
use eframe::egui::{self, Color32, FontId, Vec2};
use serde_json::Value;

pub fn color(rgb: u32) -> Color32 { Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8) }
fn asset(s: &str) -> bool { !s.is_empty() && s.len() < 200 && s.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-')) }
pub fn decoration(user: &User) -> Option<String> {
    let data = user.avatar_decoration_data.as_ref()?;
    let hash = data["asset"].as_str().filter(|s| asset(s))?;
    Some(format!("https://cdn.discordapp.com/avatar-decoration-presets/{hash}.png?size=256&passthrough=true"))
}
pub fn banner(user: &User, data: &Value, guild: Option<&str>) -> Option<String> {
    let server = data["guild_member_profile"]["banner"].as_str();
    let hash = server.or_else(|| data["user_profile"]["banner"].as_str()).or(user.banner.as_deref()).filter(|s| asset(s))?;
    let ext = if hash.starts_with("a_") { "gif" } else { "png" };
    if !crate::community::snowflake(&user.id) { return None; }
    let path = if server.is_some() {
        format!("guilds/{}/users/{}/banners", guild.filter(|g| crate::community::snowflake(g))?, user.id)
    } else { format!("banners/{}", user.id) };
    Some(format!("https://cdn.discordapp.com/{path}/{hash}.{ext}?size=1024"))
}
pub fn badge(icon: &str) -> Option<String> { asset(icon).then(|| format!("https://cdn.discordapp.com/badge-icons/{icon}.png")) }
pub fn nameplate(user: &User) -> Option<String> {
    let data = &user.collectibles.as_ref()?["nameplate"];
    if let Some(url) = data["assets"]["animated_image_url"].as_str().or_else(||data["assets"]["static_image_url"].as_str()).filter(|s|assets::public_url(s)) { return Some(url.into()); }
    let path = data["asset"].as_str()?;
    if path.len() > 200 || !path.starts_with("nameplates/") || !path.split('/').all(|s| s.is_empty() || asset(s)) { return None; }
    Some(format!("https://cdn.discordapp.com/assets/collectibles/{}img.png", if path.ends_with('/') { path.to_owned() } else { format!("{path}/") }))
}
pub fn guild_tag(user: &User) -> Option<(&str, Option<String>)> {
    let data = user.primary_guild.as_ref()?;
    if data["identity_enabled"].as_bool() != Some(true) { return None; }
    let tag = data["tag"].as_str().filter(|t| !t.is_empty() && t.chars().count() <= 8)?;
    let image = data["identity_guild_id"].as_str().filter(|g|crate::community::snowflake(g)).zip(data["badge"].as_str().filter(|s|asset(s))).map(|(g,h)|format!("https://cdn.discordapp.com/guild-tag-badges/{g}/{h}.png"));
    Some((tag, image))
}
pub fn merged(user: &User, data: &Value) -> User {
    let mut value = serde_json::to_value(user).unwrap_or_default();
    if let (Some(target), Some(source)) = (value.as_object_mut(), data["user"].as_object()) {
        for (key, value) in source { target.insert(key.clone(), value.clone()); }
    }
    serde_json::from_value(value).unwrap_or_else(|_|user.clone())
}
/// Profile artwork (banners, badges, shop art) animates only while hovered.
pub fn paint_art(ui: &egui::Ui, images: &mut assets::Images, rect: egui::Rect, url: Option<String>, radius: u8) { paint(ui, images, rect, url, radius, false) }
/// Nameplates and avatar decorations keep animating, like Discord.
pub fn paint_art_playing(ui: &egui::Ui, images: &mut assets::Images, rect: egui::Rect, url: Option<String>, radius: u8) { paint(ui, images, rect, url, radius, true) }
fn paint(ui: &egui::Ui, images: &mut assets::Images, rect: egui::Rect, url: Option<String>, radius: u8, always: bool) {
    if ui.is_rect_visible(rect) { if let Some(url) = url { if let Some(id) = images.texture_playing(&url,rect, ui.ctx(), always) { let uv=cover_uv(images.dimensions(&url,rect.size(),ui.ctx()).unwrap_or(rect.size()),rect.size());egui::Image::new((id, rect.size())).uv(uv).corner_radius(radius).paint_at(ui, rect); } } }
}
pub fn cover_uv(source:Vec2,target:Vec2)->egui::Rect{
    let source_aspect=source.x/source.y.max(1.);let target_aspect=target.x/target.y.max(1.);
    let size=if source_aspect>target_aspect{Vec2::new(target_aspect/source_aspect,1.)}else{Vec2::new(1.,source_aspect/target_aspect)};
    egui::Rect::from_center_size(egui::pos2(0.5,0.5),size)
}
/// Rounded rectangle with a top-to-bottom gradient. A fan from the centre interpolates a linear
/// vertical ramp exactly, so no banding rows are needed. Transparent ends make fades.
pub fn vertical_gradient(painter: &egui::Painter, rect: egui::Rect, top: Color32, bottom: Color32, radius: f32) {
    painter.add(vertical_gradient_shape(rect, top, bottom, radius));
}
/// The shape behind `vertical_gradient`, for painting into a placeholder slot behind content.
pub fn vertical_gradient_shape(rect: egui::Rect, top: Color32, bottom: Color32, radius: f32) -> egui::Shape {
    let r=radius.min(rect.width()/2.).min(rect.height()/2.).max(0.);
    let at=|p:egui::Pos2|top.lerp_to_gamma(bottom,((p.y-rect.top())/rect.height().max(1.)).clamp(0.,1.));
    let mut mesh=egui::Mesh::default();mesh.colored_vertex(rect.center(),at(rect.center()));
    let centers=[egui::pos2(rect.right()-r,rect.top()+r),egui::pos2(rect.right()-r,rect.bottom()-r),egui::pos2(rect.left()+r,rect.bottom()-r),egui::pos2(rect.left()+r,rect.top()+r)];
    for (corner,center) in centers.into_iter().enumerate(){for step in 0..=8{let angle=(corner as f32-1.)*std::f32::consts::FRAC_PI_2+step as f32/8.*std::f32::consts::FRAC_PI_2;let p=center+Vec2::angled(angle)*r;mesh.colored_vertex(p,at(p));}}
    let count=mesh.vertices.len()as u32-1;for i in 1..=count{mesh.add_triangle(0,i,if i==count{1}else{i+1});}
    egui::Shape::mesh(mesh)
}
pub fn gradient(ui: &egui::Ui, rect: egui::Rect, a: Color32, b: Color32) {
    let mut mesh = egui::Mesh::default();
    let r=14_f32.min(rect.width()/2.).min(rect.height()/2.);
    mesh.colored_vertex(rect.center(),a.lerp_to_gamma(b,0.5));
    let corners=[egui::pos2(rect.right()-r,rect.top()+r),egui::pos2(rect.right()-r,rect.bottom()-r),egui::pos2(rect.left()+r,rect.bottom()-r),egui::pos2(rect.left()+r,rect.top()+r)];
    for (corner,center) in corners.into_iter().enumerate(){for step in 0..=8{let angle=(corner as f32-1.)*std::f32::consts::FRAC_PI_2+step as f32/8.*std::f32::consts::FRAC_PI_2;let p=center+Vec2::angled(angle)*r;mesh.colored_vertex(p,a.lerp_to_gamma(b,((p.x-rect.left())/rect.width().max(1.)).clamp(0.,1.)));}}
    let count=mesh.vertices.len()as u32-1;for i in 1..=count{mesh.add_triangle(0,i,if i==count{1}else{i+1});}ui.painter().add(mesh);
}
pub fn name(ui: &mut egui::Ui, user: &User, label: &str, size: f32, fallback: Color32) -> egui::Response {
    let data = user.display_name_styles.as_ref().unwrap_or(&Value::Null);
    let font = data["font_id"].as_u64().unwrap_or(11);
    let family = format!("discord-name-{font}");
    let supported = matches!(font,1|2|3|4|6|8|9|10|12..=16);
    let font = if supported { FontId::new(size, egui::FontFamily::Name(family.into())) } else { FontId::proportional(size) };
    let colors: Vec<_> = data["colors"].as_array().into_iter().flatten().filter_map(Value::as_u64).take(5).map(|c|color(c as u32)).collect();
    let effect = data["effect_id"].as_u64().unwrap_or(1);
    // Display-name effects keep moving (paused while the window is unfocused).
    let animated = ui.ctx().input(|i|i.focused) && ui.ctx().data(|d|d.get_temp::<bool>(egui::Id::new("eclipse-name-animation")).unwrap_or(true)) && matches!(effect, 7 | 8);
    let t = if animated { ui.ctx().input(|i|i.time) as f32 * 0.25 } else { 0. };
    let mut job = egui::text::LayoutJob::default();
    let count = label.chars().count().max(1) as f32;
    for (i,c) in label.chars().enumerate() {
        let color = if colors.len()>1 && matches!(effect, 2|6|7|8) {
            let x = if animated{((i as f32/count+t).rem_euclid(1.))*colors.len()as f32}else{(i as f32/(count-1.).max(1.))*(colors.len()-1)as f32};
            let left=(x.floor()as usize).min(colors.len()-1);let right=if animated{(left+1)%colors.len()}else{(left+1).min(colors.len()-1)};let blend=x.fract();
            let a=colors[left].to_array();let b=colors[right].to_array(); Color32::from_rgb((a[0] as f32*(1.-blend)+b[0]as f32*blend)as u8,(a[1] as f32*(1.-blend)+b[1]as f32*blend)as u8,(a[2] as f32*(1.-blend)+b[2]as f32*blend)as u8)
        } else { colors.first().copied().unwrap_or(fallback) };
        job.append(&c.to_string(), 0., egui::TextFormat { font_id:font.clone(), color, ..Default::default() });
    }
    job.wrap.max_width = ui.available_width().max(1.); job.wrap.max_rows=1;job.wrap.break_anywhere=true;
    let galley=ui.fonts(|f|f.layout_job(job));let (rect,response)=ui.allocate_exact_size(galley.size(), egui::Sense::click());
    let painter=ui.painter().with_clip_rect(rect.expand(3.).intersect(ui.clip_rect()));
    if effect==3 {for offset in [Vec2::new(-1.5,0.),Vec2::new(1.5,0.),Vec2::new(0.,-1.5),Vec2::new(0.,1.5)] {painter.galley_with_override_text_color(rect.min+offset,galley.clone(),colors.first().copied().unwrap_or(fallback).gamma_multiply(0.35));}}
    if matches!(effect,4|5) { painter.galley_with_override_text_color(rect.min+Vec2::new(1.5,1.5),galley.clone(),Color32::BLACK); }
    if effect==8&&animated&&ui.is_rect_visible(rect){
        // Keep glyph UVs in texels until egui tessellates the complete frame. The
        // font atlas can grow while later names are laid out in this same frame.
        let mut moving=(*galley).clone();
        for placed in &mut moving.rows{let row=std::sync::Arc::make_mut(&mut placed.row);let center=row.size.y*0.5;
            for vertex in &mut row.visuals.mesh.vertices{let phase=vertex.pos.x/size.max(1.);let stretch=1.+0.085*(t*std::f32::consts::TAU*2.+phase*0.6).sin();vertex.pos.y=center+(vertex.pos.y-center)*stretch;}
            row.visuals.mesh_bounds=row.visuals.mesh_bounds.expand(size*0.1);
        }
        painter.galley(rect.min,std::sync::Arc::new(moving),fallback);
    }else{painter.galley(rect.min,galley, fallback);}
    if animated && ui.is_rect_visible(rect) { ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(1. / ui.ctx().data(|d|d.get_temp::<u32>(egui::Id::new("eclipse-animation-fps")).unwrap_or(60)).clamp(5,60) as f64)); }
    response
}

#[cfg(test)] mod tests {
    use super::*;
    use serde_json::json;
    #[test]fn gummy_name_motion_is_finite_moves_without_hover_pauses_unfocused_and_reduced_motion_disables_it(){
        let ctx=egui::Context::default();crate::widgets::fonts(&ctx);
        let user=User{display_name_styles:Some(json!({"font_id":3,"effect_id":8,"colors":[16711680,65280,255]})),..Default::default()};
        let run=|time:f64,focused:bool|{
            let output=ctx.run(egui::RawInput{time:Some(time),focused,..Default::default()},|ctx|{egui::CentralPanel::default().show(ctx,|ui|{name(ui,&user,"Smooth name",22.,Color32::WHITE);});});
            let points:Vec<_>=output.shapes.into_iter().filter_map(|s|if let egui::Shape::Text(text)=s.shape{if text.galley.job.text=="Smooth name"{Some(text.galley.rows.iter().flat_map(|r|r.visuals.mesh.vertices.iter().map(|v|v.pos)).collect::<Vec<_>>())}else{None}}else{None}).flatten().collect();assert!(!points.is_empty());assert!(points.iter().all(|p|p.x.is_finite()&&p.y.is_finite()));points};
        run(0.5,true);assert_ne!(run(1.,true),run(1.25,true));
        run(1.5,false);assert_eq!(run(2.,false),run(2.25,false));
        ctx.data_mut(|d|d.insert_temp(egui::Id::new("eclipse-name-animation"),false));
        let output=ctx.run(Default::default(),|ctx|{egui::CentralPanel::default().show(ctx,|ui|{name(ui,&user,"Smooth name",22.,Color32::WHITE);});});
        assert!(output.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.job.text=="Smooth name")));
    }
    #[test] fn profile_assets_are_scoped_and_cannot_escape_cdn_paths() {
        let user=User{id:"123456789012345678".into(),banner:Some("a_global".into()),..Default::default()};
        assert!(banner(&user,&json!({}),None).unwrap().contains("/banners/123456789012345678/a_global.gif"));
        let data=json!({"guild_member_profile":{"banner":"server"}});
        assert!(banner(&user,&data,Some("987654321098765432")).unwrap().contains("/guilds/987654321098765432/users/"));
        assert!(banner(&user,&data,None).is_none());
        let mut user=user;user.collectibles=Some(json!({"nameplate":{"asset":"nameplates/../../evil/"}}));assert!(nameplate(&user).is_none());
        user.avatar_decoration_data=Some(json!({"asset":"@evil/x"}));assert!(decoration(&user).is_none());
    }
    #[test] fn partial_profile_does_not_erase_identity_and_tags_require_opt_in() {
        let user=User{id:"1".into(),username:"person".into(),avatar:Some("avatar".into()),..Default::default()};
        let mut merged=merged(&user,&json!({"user":{"id":"1","global_name":"Name"}}));assert_eq!(merged.avatar,user.avatar);assert_eq!(merged.name(),"Name");
        merged.primary_guild=Some(json!({"identity_enabled":false,"tag":"TEST"}));assert!(guild_tag(&merged).is_none());
    }
}
