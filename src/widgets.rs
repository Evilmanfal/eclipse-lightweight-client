use eframe::egui::{self, Color32, FontId, Vec2};

#[derive(Clone,Copy)]pub enum Control {Mic,Headphones,Camera,Screen,Hangup,Activity,Settings,Emoji,Gif,Upload,Edit,Delete}
pub fn control(ui:&mut egui::Ui,kind:Control,active:bool,size:f32,tooltip:&str)->egui::Response{
    let (_,response)=ui.allocate_exact_size(Vec2::splat(size),egui::Sense::click());
    paint_control(ui,kind,active,size,tooltip,response,false)
}
/// Borderless variant for the user panel: no circle, a soft hover square and red when muted/deafened.
pub fn bar_control(ui:&mut egui::Ui,kind:Control,active:bool,size:f32,tooltip:&str)->egui::Response{
    let (_,response)=ui.allocate_exact_size(Vec2::splat(size),egui::Sense::click());
    paint_control(ui,kind,active,size,tooltip,response,true)
}
/// Small chevron that sits beside a user-panel control and opens its menu.
pub fn chevron(ui:&mut egui::Ui,height:f32,tooltip:&str)->egui::Response{
    let (rect,response)=ui.allocate_exact_size(Vec2::new(12.,height),egui::Sense::click());
    if response.hovered(){ui.painter().rect_filled(rect,4,Color32::from_white_alpha(18));}
    let color=if response.hovered(){Color32::from_gray(240)}else{Color32::from_gray(190)};let c=rect.center();let stroke=egui::Stroke::new(1.5_f32,color);
    ui.painter().line_segment([c+Vec2::new(-3.5,-1.5),c+Vec2::new(0.,2.)],stroke);ui.painter().line_segment([c+Vec2::new(0.,2.),c+Vec2::new(3.5,-1.5)],stroke);
    response.on_hover_text(tooltip)
}
#[derive(Clone,Copy)]pub enum HeaderIcon {Pin,People,Message,Profile}
/// Small borderless conversation-header icon; `color` lets callers show an active state.
pub fn header_icon(ui:&mut egui::Ui,kind:HeaderIcon,color:Color32,tooltip:&str)->egui::Response{
    let size=28.;let (rect,response)=ui.allocate_exact_size(Vec2::splat(size),egui::Sense::click());
    if response.hovered(){ui.painter().rect_filled(rect,7,Color32::from_gray(40));}
    let color=if response.hovered(){color.lerp_to_gamma(Color32::WHITE,0.35)}else{color};
    paint_icon(ui.painter(),rect.center(),size/28.,kind,egui::Stroke::new(1.7_f32,color));
    response.on_hover_text(tooltip)
}
/// Round icon button used in list rows (Friends): dark circle, lighter on hover.
pub fn round_icon(ui:&mut egui::Ui,kind:HeaderIcon,tooltip:&str)->egui::Response{
    let size=32.;let (rect,response)=ui.allocate_exact_size(Vec2::splat(size),egui::Sense::click());
    ui.painter().circle_filled(rect.center(),size/2.,Color32::from_gray(if response.hovered(){52}else{32}));
    paint_icon(ui.painter(),rect.center(),0.72,kind,egui::Stroke::new(1.6_f32,Color32::from_gray(if response.hovered(){245}else{200})));
    response.on_hover_text(tooltip)
}
fn paint_icon(painter:&egui::Painter,center:egui::Pos2,unit:f32,kind:HeaderIcon,stroke:egui::Stroke){
    let p=|x:f32,y:f32|center+Vec2::new(x,y)*unit;
    let arc=|cx:f32,cy:f32,r:f32,from:f32,to:f32|(0..=12).map(|i|{let a=from+(to-from)*i as f32/12.;p(cx+r*a.cos(),cy+r*a.sin())}).collect::<Vec<_>>();
    use std::f32::consts::{PI,TAU};
    match kind{
        HeaderIcon::Pin=>{
            let (sin,cos)=std::f32::consts::FRAC_PI_4.sin_cos();let q=|x:f32,y:f32|center+Vec2::new(x*cos-y*sin,x*sin+y*cos)*unit;
            for (a,b) in [((-5.,-10.),(5.,-10.)),((-3.,-10.),(-3.,-3.)),((3.,-10.),(3.,-3.)),((-3.,-3.),(-7.,2.)),((3.,-3.),(7.,2.)),((-7.,2.),(7.,2.)),((0.,2.),(0.,10.))]{painter.line_segment([q(a.0,a.1),q(b.0,b.1)],stroke);}
        },
        HeaderIcon::People=>{
            painter.circle_stroke(p(-3.,-4.),4.*unit,stroke);painter.add(egui::Shape::line(arc(-3.,9.,7.,PI,TAU),stroke));
            painter.circle_stroke(p(6.,-6.),3.*unit,stroke);painter.add(egui::Shape::line(arc(6.,5.,5.,1.6*PI,TAU),stroke));
        },
        HeaderIcon::Message=>{
            // Speech bubble: most of a circle with a tail at the lower left.
            painter.add(egui::Shape::line(arc(1.,-1.,8.,0.75*PI,2.6*PI),stroke));
            painter.line_segment([p(1.+8.*(0.75*PI).cos(),-1.+8.*(0.75*PI).sin()),p(-8.,8.)],stroke);
            painter.line_segment([p(-8.,8.),p(1.+8.*(0.6*PI).cos(),-1.+8.*(0.6*PI).sin())],stroke);
        },
        HeaderIcon::Profile=>{painter.circle_stroke(p(0.,-4.),4.*unit,stroke);painter.add(egui::Shape::line(arc(0.,9.,7.,PI,TAU),stroke));},
    }
}
pub fn control_at(ui:&mut egui::Ui,rect:egui::Rect,id:egui::Id,kind:Control,tooltip:&str)->egui::Response{
    let response=ui.interact(rect,id,egui::Sense::click());
    paint_control(ui,kind,false,rect.width(),tooltip,response,false)
}
fn paint_control(ui:&egui::Ui,kind:Control,active:bool,size:f32,tooltip:&str,response:egui::Response,plain:bool)->egui::Response{
    let rect=response.rect;let center=rect.center();
    let danger=matches!(kind,Control::Hangup);let fill=if danger{Color32::from_rgb(218,55,60)}else if active{Color32::from_gray(242)}else if response.hovered(){Color32::from_gray(74)}else{Color32::from_gray(46)};
    let color=if plain{
        if response.hovered(){ui.painter().rect_filled(rect,6,Color32::from_white_alpha(18));}
        if active&&matches!(kind,Control::Mic|Control::Headphones){Color32::from_rgb(242,63,67)}else if response.hovered(){Color32::from_gray(245)}else{Color32::from_gray(205)}
    }else{
        ui.painter().circle_filled(center,size*0.48,fill);
        if matches!(kind,Control::Delete){Color32::from_rgb(240,132,140)}else if active&&!danger{Color32::from_gray(20)}else{Color32::from_gray(232)}
    };let stroke=egui::Stroke::new(1.8_f32,color);let unit=if plain{size/32.}else{size/44.};let p=|x:f32,y:f32|center+Vec2::new(x*unit,y*unit);let line=|a:(f32,f32),b:(f32,f32)|{ui.painter().line_segment([p(a.0,a.1),p(b.0,b.1)],stroke);};
    match kind{
        Control::Mic=>{ui.painter().rect_stroke(egui::Rect::from_two_pos(p(-3.,-9.),p(3.,3.)),4,stroke,egui::StrokeKind::Inside);line((-7.,-1.),(-7.,4.));line((-7.,4.),(0.,8.));line((0.,8.),(7.,4.));line((7.,4.),(7.,-1.));line((0.,8.),(0.,12.));line((-4.,12.),(4.,12.));},
        Control::Headphones=>{line((-9.,8.),(-9.,-3.));line((-9.,-3.),(-5.,-8.));line((-5.,-8.),(5.,-8.));line((5.,-8.),(9.,-3.));line((9.,-3.),(9.,8.));ui.painter().rect_filled(egui::Rect::from_two_pos(p(-11.,2.),p(-6.,10.)),2,color);ui.painter().rect_filled(egui::Rect::from_two_pos(p(6.,2.),p(11.,10.)),2,color);},
        Control::Camera=>{ui.painter().rect_stroke(egui::Rect::from_two_pos(p(-10.,-6.),p(4.,7.)),2,stroke,egui::StrokeKind::Inside);line((4.,-2.),(11.,-6.));line((11.,-6.),(11.,7.));line((11.,7.),(4.,3.));},
        Control::Screen=>{ui.painter().rect_stroke(egui::Rect::from_two_pos(p(-11.,-8.),p(11.,6.)),2,stroke,egui::StrokeKind::Inside);line((0.,6.),(0.,11.));line((-6.,11.),(6.,11.));line((0.,3.),(0.,-5.));line((0.,-5.),(-4.,-1.));line((0.,-5.),(4.,-1.));},
        Control::Hangup=>{line((-11.,0.),(-6.,-4.));line((-6.,-4.),(6.,-4.));line((6.,-4.),(11.,0.));line((-10.,0.),(-10.,5.));line((10.,0.),(10.,5.));},
        Control::Activity=>{ui.painter().rect_stroke(egui::Rect::from_two_pos(p(-11.,-6.),p(11.,8.)),4,stroke,egui::StrokeKind::Inside);line((-8.,0.),(-2.,0.));line((-5.,-3.),(-5.,3.));ui.painter().circle_filled(p(5.,0.),1.5*unit,color);ui.painter().circle_filled(p(8.,3.),1.5*unit,color);},
        Control::Emoji=>{ui.painter().circle_stroke(center,10.*unit,stroke);ui.painter().circle_filled(p(-3.5,-3.),1.4*unit,color);ui.painter().circle_filled(p(3.5,-3.),1.4*unit,color);line((-5.,3.),(-2.,6.));line((-2.,6.),(2.,6.));line((2.,6.),(5.,3.));},
        Control::Gif=>{ui.painter().rect_stroke(egui::Rect::from_two_pos(p(-12.,-8.),p(12.,8.)),2,stroke,egui::StrokeKind::Inside);ui.painter().text(center,egui::Align2::CENTER_CENTER,"GIF",FontId::proportional(11.*unit),color);},
        Control::Upload=>{line((-10.,0.),(10.,0.));line((0.,-10.),(0.,10.));},
        Control::Edit=>{line((-9.,9.),(-6.,1.));line((-6.,1.),(6.,-11.));line((6.,-11.),(11.,-6.));line((11.,-6.),(-1.,6.));line((-1.,6.),(-9.,9.));line((3.,-8.),(8.,-3.));},
        Control::Delete=>{line((-10.,-7.),(10.,-7.));line((-4.,-7.),(-4.,-11.));line((-4.,-11.),(4.,-11.));line((4.,-11.),(4.,-7.));line((-7.,-3.),(-6.,10.));line((-6.,10.),(6.,10.));line((6.,10.),(7.,-3.));line((-2.,-2.),(-2.,6.));line((2.,-2.),(2.,6.));},
        Control::Settings=>{ui.painter().circle_stroke(center,7.*unit,stroke);ui.painter().circle_stroke(center,2.*unit,stroke);for i in 0..8{let a=i as f32*std::f32::consts::TAU/8.;let d=Vec2::angled(a);ui.painter().line_segment([center+d*8.*unit,center+d*11.*unit],stroke);}},
    }
    if active&&matches!(kind,Control::Mic|Control::Headphones|Control::Activity){ui.painter().line_segment([p(-12.,12.),p(12.,-12.)],egui::Stroke::new(2.4_f32,Color32::from_rgb(235,75,85)));}
    response.on_hover_text(tooltip)
}

pub fn fonts(ctx: &egui::Context) {
    fonts_with_emoji(ctx,false);
}
pub fn fonts_with_emoji(ctx: &egui::Context,extra_emoji:bool) {
    let mut fonts = egui::FontDefinitions::default();
    for (id, bytes) in [
        (1, include_bytes!("../assets/fonts/Bangers.ttf").as_slice()),
        (2, include_bytes!("../assets/fonts/BioRhyme.ttf").as_slice()),
        (3, include_bytes!("../assets/fonts/CherryBombOne.ttf").as_slice()),
        (4, include_bytes!("../assets/fonts/Chicle.ttf").as_slice()),
        (6, include_bytes!("../assets/fonts/MuseoModerno.ttf").as_slice()),
        (8, include_bytes!("../assets/fonts/PixelifySans.ttf").as_slice()),
        (9, include_bytes!("../assets/fonts/Ribes.ttf").as_slice()),
        (10, include_bytes!("../assets/fonts/Sinistre.ttf").as_slice()),
        (12, include_bytes!("../assets/fonts/ZillaSlab.ttf").as_slice()),
        (13, include_bytes!("../assets/fonts/PlaypenSans.ttf").as_slice()),
        (14, include_bytes!("../assets/fonts/Orbitron.ttf").as_slice()),
        (15, include_bytes!("../assets/fonts/NewRocker.ttf").as_slice()),
        (16, include_bytes!("../assets/fonts/Kalam.ttf").as_slice()),
    ] {
        let name=format!("discord-name-{id}");
        fonts.font_data.insert(name.clone(),egui::FontData::from_owned(bytes.to_vec()).into());
        let mut fallback=vec![name.clone()];fallback.extend(fonts.families[&egui::FontFamily::Proportional].clone());
        fonts.families.insert(egui::FontFamily::Name(name.into()),fallback);
    }
    // Read installed Windows fonts. No system font is copied into the distribution.
    for (name, file) in [
        ("Segoe UI", "segoeui.ttf"),
        ("Segoe Symbols", "seguisym.ttf"),
        ("Segoe Historic", "seguihis.ttf"),
    ] {
        if let Ok(bytes) = std::fs::read(std::path::Path::new("C:/Windows/Fonts").join(file)) {
            fonts
                .font_data
                .insert(name.into(), egui::FontData::from_owned(bytes).into());
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                if name=="Segoe UI" && family==egui::FontFamily::Proportional {
                    fonts.families.entry(family).or_default().insert(0,name.into());
                } else { fonts.families.entry(family).or_default().push(name.into()); }
            }
        }
    }
    if extra_emoji {if let Ok(bytes)=std::fs::read("C:/Windows/Fonts/seguiemj.ttf") {
        fonts.font_data.insert("Segoe Emoji".into(),egui::FontData::from_owned(bytes).into());
        fonts.families.entry(egui::FontFamily::Proportional).or_default().push("Segoe Emoji".into());
    }}
    for file in ["msjhl.ttc", "msjh.ttc", "msgothic.ttc", "malgun.ttf"] {
        if let Ok(bytes) = std::fs::read(std::path::Path::new("C:/Windows/Fonts").join(file)) {
            fonts.font_data.insert(
                "Windows CJK".into(),
                egui::FontData::from_owned(bytes).into(),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push("Windows CJK".into());
            break;
        }
    }
    let fallback=fonts.families[&egui::FontFamily::Proportional].clone();
    for (family,names) in &mut fonts.families {if matches!(family,egui::FontFamily::Name(_)){for name in &fallback {if !names.contains(name){names.push(name.clone());}}}}
    ctx.set_fonts(fonts);
}
pub fn read_all(ui:&mut egui::Ui)->egui::Response {
    let (rect,response)=ui.allocate_exact_size(Vec2::new(48.,32.),egui::Sense::click());
    if response.hovered(){ui.painter().rect_filled(rect,8,Color32::from_gray(42));}
    ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,"Read All",FontId::proportional(11.),Color32::from_gray(224));
    response.on_hover_text("Read all notifications")
}
pub fn needs_emoji_font<'a>(ctx:&egui::Context,texts:impl Iterator<Item=&'a str>)->bool {
    let mut seen=std::collections::HashSet::new();
    ctx.fonts(|fonts|texts.flat_map(str::chars).filter(|c|(0x1f000..=0x1faff).contains(&(*c as u32))).any(|c|seen.insert(c)&&!fonts.has_glyph(&FontId::proportional(14.0),c)))
}

/// Paint the title into the parent's click region: no child label can steal clicks.
pub fn row_text(ui: &egui::Ui, rect: egui::Rect, left: f32, text: &str, color: Color32) {
    let mut job = egui::text::LayoutJob::simple(
        text.to_owned(),
        FontId::proportional(14.0),
        color,
        (rect.width() - left - 10.0).max(1.0),
    );
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = ui.fonts(|fonts| fonts.layout_job(job));
    let position = egui::pos2(rect.left() + left, rect.center().y - galley.size().y / 2.0);
    ui.painter()
        .with_clip_rect(rect.intersect(ui.clip_rect()))
        .galley(position, galley, color);
}
pub fn nav_row(ui: &mut egui::Ui, text: &str, selected: bool, color: Color32) -> egui::Response {
    navigation_row(ui,text,selected,color,35.0)
}
pub fn home_row(ui: &mut egui::Ui, text: &str, selected: bool, color: Color32) -> egui::Response {
    navigation_row(ui,text,selected,color,30.0)
}
pub fn channel_row(ui: &mut egui::Ui, text: &str, selected: bool, color: Color32) -> egui::Response {
    navigation_row(ui,text,selected,color,30.0)
}
fn navigation_row(ui: &mut egui::Ui, text: &str, selected: bool, color: Color32,height:f32) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), egui::Sense::click());
    if selected || response.hovered() {
        ui.painter()
            .rect_filled(rect, 18, Color32::from_gray(if selected { 49 } else { 35 }));
    }
    row_text(ui, rect, 10.0, text, color);
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_name_fonts_are_readable_and_keep_unicode_fallbacks() {
        let ctx=egui::Context::default();fonts(&ctx);
        let _=ctx.run(Default::default(),|ctx|{ctx.fonts(|fonts|{for id in [1,2,3,4,6,8,9,10,12,13,14,15,16]{let font=FontId::new(18.,egui::FontFamily::Name(format!("discord-name-{id}").into()));assert!(fonts.has_glyph(&font,'A'),"font {id}");assert!(fonts.has_glyph(&font,'λ'),"fallback {id}");}});});
    }
    #[test]
    fn decorated_channel_names_have_glyphs() {
        let ctx = egui::Context::default();
        fonts(&ctx);
        let _ = ctx.run(Default::default(), |ctx| {
            ctx.fonts(|fonts| {
                let font = FontId::proportional(14.0);
                for c in "🐾・𝐠𝐞𝐧𝐞𝐫𝐚𝐥📷𝓼𝓱𝓸𝔀𝓬𝓪𝓼𝓮⟡✦│「」︱".chars()
                {
                    assert!(
                        fonts.has_glyph(&font, c),
                        "missing glyph {c} U+{:04X}",
                        c as u32
                    );
                }
            })
        });
    }
    #[test]
    fn entire_navigation_row_accepts_clicks() {
        for fraction in [0.05, 0.3, 0.65, 0.95] {
            let ctx = egui::Context::default();
            let mut rect = egui::Rect::NOTHING;
            let mut clicked = false;
            for phase in 0..3 {
                let mut input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(260.0, 120.0),
                    )),
                    ..Default::default()
                };
                if phase > 0 {
                    let pos = egui::pos2(rect.left() + rect.width() * fraction, rect.center().y);
                    input.events = vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: phase == 1,
                            modifiers: Default::default(),
                        },
                    ];
                }
                let _ = ctx.run(input, |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let (r, response) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), 40.0),
                            egui::Sense::click(),
                        );
                        row_text(
                            ui,
                            r,
                            45.0,
                            "A long direct message name that must never intercept the row",
                            Color32::WHITE,
                        );
                        rect = r;
                        clicked |= response.clicked();
                    });
                });
            }
            assert!(clicked, "click at {}% was lost", fraction * 100.0);
        }
    }
}
