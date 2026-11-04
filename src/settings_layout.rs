use eframe::egui::{self,Color32,FontId,RichText,Sense,Stroke,Vec2,pos2};
const PANEL:Color32=Color32::from_gray(23);
const TITLE:Color32=Color32::from_gray(242);
const BODY:Color32=Color32::from_gray(219);
const HINT:Color32=Color32::from_gray(150);
const LINE:Color32=Color32::from_gray(40);
const FIELD:Color32=Color32::from_gray(15);
/// Discord's blurple, used whenever the active theme's accent is a neutral grey.
const BLURPLE:Color32=Color32::from_rgb(88,101,242);
fn accent_id()->egui::Id{egui::Id::new("settings-accent")}

/// Shared, bounded layout for account and server settings. The backdrop owns outside clicks.
pub fn shell(ctx:&egui::Context,id:&str,title:&str,subtitle:&str,content:impl FnOnce(&mut egui::Ui,Vec2))->bool{
    let size=Vec2::new((ctx.screen_rect().width()-100.).clamp(650.,1040.),(ctx.screen_rect().height()-88.).clamp(400.,780.));
    let mut close=false;
    let response=egui::Modal::new(egui::Id::new(id)).backdrop_color(Color32::from_black_alpha(175))
        .frame(egui::Frame::NONE.fill(PANEL).corner_radius(20).stroke(Stroke::new(1_f32,Color32::from_gray(44))))
        .show(ctx,|ui|{
            ui.set_width(size.x);ui.set_height(size.y);
            egui::Frame::NONE.inner_margin(egui::Margin{left:22,right:18,top:14,bottom:10}).show(ui,|ui|{
                ui.horizontal(|ui|{ui.vertical(|ui|{ui.label(RichText::new(title).size(20.).strong().color(TITLE));ui.label(RichText::new(subtitle).size(12.5).color(HINT));});
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if close_button(ui).on_hover_text("Close settings · Escape").clicked(){close=true;}});
                });
            });
            let (line,_)=ui.allocate_exact_size(Vec2::new(size.x,1.),Sense::hover());ui.painter().rect_filled(line,0,LINE);
            content(ui,Vec2::new(size.x,size.y-100.));
        });
    close||response.should_close()
}
/// Discord-style circular close button with an "ESC" caption underneath.
fn close_button(ui:&mut egui::Ui)->egui::Response{
    let (rect,response)=ui.allocate_exact_size(Vec2::new(40.,54.),Sense::click());
    let center=pos2(rect.center().x,rect.top()+18.);let hovered=response.hovered();
    let color=Color32::from_gray(if hovered{240}else{185});
    if hovered{ui.painter().circle_filled(center,17.,Color32::from_white_alpha(14));}
    ui.painter().circle_stroke(center,17.,Stroke::new(2_f32,color));
    let stroke=Stroke::new(2_f32,color);let d=5.;
    ui.painter().line_segment([center+Vec2::new(-d,-d),center+Vec2::new(d,d)],stroke);ui.painter().line_segment([center+Vec2::new(d,-d),center+Vec2::new(-d,d)],stroke);
    ui.painter().text(pos2(center.x,rect.bottom()),egui::Align2::CENTER_BOTTOM,"ESC",FontId::proportional(10.),color);
    response
}
/// Remember the accent the settings controls use this frame. Neutral grey themes fall back to blurple.
pub fn set_accent(ctx:&egui::Context,theme_accent:Color32){
    let [r,g,b,_]=theme_accent.to_array();let spread=r.max(g).max(b)-r.min(g).min(b);
    ctx.data_mut(|d|d.insert_temp(accent_id(),if spread<28{BLURPLE}else{theme_accent}));
}
pub fn accent(ctx:&egui::Context)->Color32{ctx.data(|d|d.get_temp(accent_id())).unwrap_or(BLURPLE)}
/// Roomier controls, rounded inputs and accent-coloured selections for settings pages.
pub fn style(ui:&mut egui::Ui){
    let accent=accent(ui.ctx());let s=ui.style_mut();
    s.spacing.item_spacing=Vec2::new(10.,8.);s.spacing.button_padding=Vec2::new(14.,7.);s.spacing.interact_size.y=34.;
    s.spacing.slider_rail_height=6.;s.spacing.combo_width=260.;
    let v=&mut s.visuals;v.extreme_bg_color=FIELD;v.selection.bg_fill=accent;v.selection.stroke=Stroke::new(1_f32,Color32::WHITE);
    v.hyperlink_color=Color32::from_rgb(0,168,252);
    for (w,fill,stroke) in [(&mut v.widgets.inactive,38,52),(&mut v.widgets.hovered,50,70),(&mut v.widgets.active,58,84)]{
        w.weak_bg_fill=Color32::from_gray(fill);w.bg_fill=Color32::from_gray(64);w.bg_stroke=Stroke::new(1_f32,Color32::from_gray(stroke));w.corner_radius=egui::CornerRadius::same(8);w.expansion=0.;
    }
    v.widgets.open.weak_bg_fill=Color32::from_gray(46);v.widgets.open.corner_radius=egui::CornerRadius::same(8);
    v.widgets.noninteractive.bg_stroke=Stroke::new(1_f32,LINE);
}
/// Page title and lead-in at the top of a settings page.
pub fn page_heading(ui:&mut egui::Ui,title:&str,description:&str){
    ui.label(RichText::new(title).size(21.).strong().color(TITLE));ui.add_space(2.);
    ui.label(RichText::new(description).size(13.5).color(HINT));ui.add_space(18.);
}
/// Bold section heading, e.g. "Input Profile".
pub fn section(ui:&mut egui::Ui,title:&str){ui.label(RichText::new(title).size(15.5).strong().color(TITLE));ui.add_space(2.);}
/// Small uppercase label above a control, e.g. "INPUT DEVICE".
pub fn label(ui:&mut egui::Ui,text:&str){ui.label(RichText::new(text.to_uppercase()).size(11.5).strong().color(HINT));}
pub fn help(ui:&mut egui::Ui,text:&str){ui.label(RichText::new(text).size(12.5).color(HINT));}
/// Thin full-width divider with breathing room above and below.
pub fn divider(ui:&mut egui::Ui){ui.add_space(10.);let (rect,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),1.),Sense::hover());ui.painter().rect_filled(rect,0,LINE);ui.add_space(14.);}

/// Animated pill switch with a check or cross in the knob.
pub fn switch(ui:&mut egui::Ui,on:&mut bool)->egui::Response{
    let (rect,mut response)=ui.allocate_exact_size(Vec2::new(42.,24.),Sense::click());
    if response.clicked(){*on=!*on;response.mark_changed();}
    let state=*on;let enabled=ui.is_enabled();
    response.widget_info(||egui::WidgetInfo::selected(egui::WidgetType::Checkbox,enabled,state,"switch"));
    let t=ui.ctx().animate_bool_with_time(response.id,state,0.12);
    let off=Color32::from_gray(if response.hovered(){100}else{82});let fill=off.lerp_to_gamma(accent(ui.ctx()),t);
    let painter=ui.painter();painter.rect_filled(rect,12,fill);
    let r=rect.height()/2.-3.;let x=egui::lerp((rect.left()+3.+r)..=(rect.right()-3.-r),t);let c=pos2(x,rect.center().y);
    painter.circle_filled(c,r,Color32::WHITE);
    let mark=Stroke::new(2_f32,fill);
    if t>0.5{painter.line_segment([c+Vec2::new(-3.6,0.2),c+Vec2::new(-1.,2.8)],mark);painter.line_segment([c+Vec2::new(-1.,2.8),c+Vec2::new(3.8,-2.6)],mark);}
    else{let d=3.;painter.line_segment([c+Vec2::new(-d,-d),c+Vec2::new(d,d)],mark);painter.line_segment([c+Vec2::new(d,-d),c+Vec2::new(-d,d)],mark);}
    response
}
/// Title + description on the left, switch on the right, divider below. The whole row toggles.
pub fn toggle_row(ui:&mut egui::Ui,title:&str,description:&str,on:&mut bool)->bool{
    let width=ui.available_width();let text_width=(width-64.).max(120.);
    let before=*on;
    ui.horizontal(|ui|{
        ui.vertical(|ui|{ui.set_width(text_width);ui.spacing_mut().item_spacing.y=3.;
            if ui.add(egui::Label::new(RichText::new(title).size(14.5).color(BODY)).sense(Sense::click())).clicked(){*on=!*on;}
            if !description.is_empty()&&ui.add(egui::Label::new(RichText::new(description).size(12.5).color(HINT)).wrap().sense(Sense::click())).clicked(){*on=!*on;}
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{switch(ui,on);});
    });
    divider(ui);
    before!=*on
}
/// Large radio choice with a description, like Discord's "Input Profile" options.
pub fn radio_row(ui:&mut egui::Ui,selected:bool,title:&str,description:&str)->bool{
    let width=ui.available_width();let wrap=width-46.;
    let title_g=ui.painter().layout(title.into(),FontId::proportional(14.5),BODY,wrap);
    let desc_g=(!description.is_empty()).then(||ui.painter().layout(description.into(),FontId::proportional(12.5),HINT,wrap));
    let height=10.+title_g.size().y+desc_g.as_ref().map_or(0.,|g|g.size().y+2.)+10.;
    let (rect,response)=ui.allocate_exact_size(Vec2::new(width,height),Sense::click());
    let painter=ui.painter();let accent=accent(ui.ctx());
    if selected{painter.rect_filled(rect,8,Color32::from_white_alpha(9));}else if response.hovered(){painter.rect_filled(rect,8,Color32::from_white_alpha(6));}
    let c=pos2(rect.left()+18.,rect.top()+10.+title_g.size().y/2.);
    painter.circle_stroke(c,9.,Stroke::new(2_f32,if selected{accent}else{Color32::from_gray(if response.hovered(){200}else{160})}));
    if selected{painter.circle_filled(c,4.5,Color32::WHITE);}
    let text_x=rect.left()+38.;let title_h=title_g.size().y;
    painter.galley(pos2(text_x,rect.top()+10.),title_g,BODY);
    if let Some(g)=desc_g{painter.galley(pos2(text_x,rect.top()+12.+title_h),g,HINT);}
    response.clicked()
}
/// Accent-filled slider with a white knob and the formatted value at the end.
pub fn slider<N:egui::emath::Numeric>(ui:&mut egui::Ui,value:&mut N,range:std::ops::RangeInclusive<N>,format:impl Fn(f64)->String)->egui::Response{
    let (min,max)=(range.start().to_f64(),range.end().to_f64());
    ui.horizontal(|ui|{
        ui.spacing_mut().slider_width=(ui.available_width()-64.).max(100.);
        let response=ui.add(egui::Slider::new(value,range).show_value(false).trailing_fill(true).handle_shape(egui::style::HandleShape::Circle));
        let rect=response.rect;let radius=rect.height()/2.5;
        let t=if max>min{((value.to_f64()-min)/(max-min)).clamp(0.,1.) as f32}else{0.};
        let x=egui::lerp((rect.left()+radius)..=(rect.right()-radius),t);
        ui.painter().circle(pos2(x,rect.center().y),9.,Color32::WHITE,Stroke::new(if response.hovered()||response.dragged(){3_f32}else{0_f32},Color32::from_white_alpha(40)));
        ui.label(RichText::new(format(value.to_f64())).size(13.).color(BODY));
        response
    }).inner
}
pub fn percent(v:f64)->String{format!("{v:.0}%")}
/// Filled accent call-to-action button.
pub fn primary(ui:&mut egui::Ui,text:&str,enabled:bool)->egui::Response{
    let accent=accent(ui.ctx());
    ui.add_enabled(enabled,egui::Button::new(RichText::new(text).strong().color(Color32::WHITE)).fill(accent).stroke(Stroke::NONE).corner_radius(8).min_size(Vec2::new(0.,36.)))
}
/// A single keyboard key drawn as a raised pill.
pub fn key_pill(ui:&mut egui::Ui,text:&str){
    egui::Frame::NONE.fill(Color32::from_gray(40)).stroke(Stroke::new(1_f32,Color32::from_gray(62))).corner_radius(6).inner_margin(egui::Margin::symmetric(8,3))
        .show(ui,|ui|{ui.label(RichText::new(text).size(12.5).monospace().color(BODY));});
}
/// Action on the left, key chord (split on "+") as pills on the right.
pub fn keybind_row(ui:&mut egui::Ui,action:&str,chord:&str){
    ui.horizontal(|ui|{ui.set_min_height(30.);ui.label(RichText::new(action).size(14.).color(BODY));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{for key in chord.split('+').rev(){key_pill(ui,key);}});});
    ui.add_space(2.);let (rect,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),1.),Sense::hover());ui.painter().rect_filled(rect,0,LINE);ui.add_space(4.);
}
/// Selectable option card with a title and caption; used for presets.
pub fn option_card(ui:&mut egui::Ui,width:f32,selected:bool,title:&str,caption:&str)->bool{
    let (rect,response)=ui.allocate_exact_size(Vec2::new(width,64.),Sense::click());let accent=accent(ui.ctx());let painter=ui.painter();
    painter.rect(rect,10,Color32::from_gray(if response.hovered(){36}else{30}),Stroke::new(if selected{2_f32}else{1_f32},if selected{accent}else{Color32::from_gray(48)}),egui::StrokeKind::Inside);
    painter.text(rect.left_top()+Vec2::new(14.,13.),egui::Align2::LEFT_TOP,title,FontId::proportional(14.5),TITLE);
    painter.text(rect.left_top()+Vec2::new(14.,35.),egui::Align2::LEFT_TOP,caption,FontId::proportional(12.),HINT);
    if selected{let c=pos2(rect.right()-16.,rect.top()+16.);painter.circle_filled(c,7.,accent);let s=Stroke::new(1.8_f32,Color32::WHITE);painter.line_segment([c+Vec2::new(-3.,0.),c+Vec2::new(-0.8,2.4)],s);painter.line_segment([c+Vec2::new(-0.8,2.4),c+Vec2::new(3.2,-2.2)],s);}
    response.clicked()
}
/// Theme preview tile: a tiny mock window in the theme's colours.
pub fn theme_swatch(ui:&mut egui::Ui,selected:bool,name:&str,background:Color32,surface:Color32,accent_color:Color32,text:Color32)->bool{
    let (rect,response)=ui.allocate_exact_size(Vec2::new(132.,104.),Sense::click());let accent=accent(ui.ctx());let painter=ui.painter();
    let preview=egui::Rect::from_min_size(rect.min,Vec2::new(rect.width(),76.));
    painter.rect_filled(preview,10,background);
    painter.rect_filled(egui::Rect::from_min_size(preview.min,Vec2::new(34.,preview.height())),egui::CornerRadius{nw:10,sw:10,ne:0,se:0},surface);
    for i in 0..3{painter.circle_filled(preview.min+Vec2::new(17.,15.+i as f32*20.),7.,if i==0{accent_color}else{text.gamma_multiply(0.25)});}
    for (i,w) in [60.,44.,72.].iter().enumerate(){painter.rect_filled(egui::Rect::from_min_size(preview.min+Vec2::new(44.,14.+i as f32*18.),Vec2::new(*w,7.)),3,text.gamma_multiply(if i==0{0.9}else{0.4}));}
    painter.rect_filled(egui::Rect::from_min_size(preview.min+Vec2::new(44.,60.),Vec2::new(36.,8.)),4,accent_color);
    painter.rect_stroke(preview,10,Stroke::new(if selected{2.5_f32}else if response.hovered(){1.5_f32}else{1_f32},if selected{accent}else{Color32::from_gray(if response.hovered(){90}else{52})}),egui::StrokeKind::Outside);
    painter.text(pos2(rect.left()+2.,rect.bottom()-3.),egui::Align2::LEFT_BOTTOM,name,FontId::proportional(13.),if selected{TITLE}else{BODY});
    response.clicked()
}
/// Selectable list entry (roles, channels) with an optional colour dot and caption.
pub fn list_row(ui:&mut egui::Ui,title:&str,caption:&str,dot:Option<Color32>,selected:bool)->egui::Response{
    let height=if caption.is_empty(){38.}else{48.};
    let (rect,response)=ui.allocate_exact_size(Vec2::new(ui.available_width(),height),Sense::click());let painter=ui.painter();
    if selected{painter.rect_filled(rect,8,Color32::from_gray(48));}else if response.hovered(){painter.rect_filled(rect,8,Color32::from_gray(34));}
    let mut x=rect.left()+12.;
    if let Some(color)=dot{painter.circle_filled(pos2(x+6.,rect.center().y),6.,color);x+=22.;}
    let title_y=if caption.is_empty(){rect.center().y}else{rect.top()+16.};
    painter.text(pos2(x,title_y),egui::Align2::LEFT_CENTER,title,FontId::proportional(14.5),if selected||response.hovered(){TITLE}else{BODY});
    if !caption.is_empty(){painter.text(pos2(x,rect.top()+34.),egui::Align2::LEFT_CENTER,caption,FontId::proportional(12.),HINT);}
    ui.add_space(2.);
    response
}
/// Discord-style dropdown entry: full-width, rounded hover highlight, optional trailing glyph and danger colouring.
pub fn menu_item(ui:&mut egui::Ui,text:&str,icon:&str,danger:bool)->egui::Response{
    let width=ui.available_width().max(200.);
    let (rect,response)=ui.allocate_exact_size(Vec2::new(width,34.),Sense::click());let painter=ui.painter();
    let red=Color32::from_rgb(242,95,100);let hovered=response.hovered();
    if hovered{painter.rect_filled(rect,6,if danger{red}else{accent(ui.ctx())});}
    let color=if hovered{Color32::WHITE}else if danger{red}else{BODY};
    painter.text(pos2(rect.left()+10.,rect.center().y),egui::Align2::LEFT_CENTER,text,FontId::proportional(14.),color);
    if !icon.is_empty(){painter.text(pos2(rect.right()-10.,rect.center().y),egui::Align2::RIGHT_CENTER,icon,FontId::proportional(14.),color);}
    response
}
pub fn menu_separator(ui:&mut egui::Ui){ui.add_space(3.);let (rect,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),1.),Sense::hover());ui.painter().rect_filled(rect.shrink2(Vec2::new(6.,0.)),0,Color32::from_gray(48));ui.add_space(3.);}
pub fn card(ui:&mut egui::Ui,content:impl FnOnce(&mut egui::Ui)){
    ui.vertical(|ui|{ui.set_width(ui.available_width());style(ui);content(ui);});
}
pub fn description(page:&str)->&'static str{match page{
    "Account & Profile"=>"Make your profile feel like you.","Voice & Video"=>"Choose your devices, input mode and call quality.",
    "Appearance"|"Themes"=>"Make Eclipse comfortable for your eyes.","Performance"=>"Balance smooth animations and memory use.",
    "Devices"=>"Review the devices signed into your account.","Connections"=>"Your connected services and accounts.",
    "Authorized Apps"=>"Apps you have allowed to access your account.","Keybinds"=>"Shortcuts that keep you in control.",
    "Notifications"=>"Choose what you hear and what gets your attention.","Plugins"=>"Optional native features, built into Eclipse.",
    "Chat"=>"How messages, media and names are shown.","Accessibility"=>"Motion, contrast and text size.",
    "Content & Social"=>"Control who can reach you and what you see.","Activity Privacy"=>"Choose what others can see about your activity.",
    "Streamer Mode"=>"Hide personal details while you stream.","Advanced"=>"Tools for power users and developers.",
    "Roles"=>"Organize your community and choose what each role can do.","Overview"=>"Your server's identity and everyday defaults.",
    "Members"=>"Manage your community and member roles.","Channels"=>"Organize the places where your community talks.",
    _=>"Your preferences, all in one place.",
}}
#[cfg(test)]mod tests{use super::*;
    #[test]fn settings_backdrop_dismisses_but_content_click_does_not(){
        let ctx=egui::Context::default();let mut rect=egui::Rect::NOTHING;
        let input=|events|egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,Vec2::new(1080.,680.))),events,..Default::default()};
        for _ in 0..3{let _=ctx.run(input(vec![]),|ctx|{assert!(!shell(ctx,"test-settings","Settings","Preferences",|ui,_|{rect=ui.max_rect();ui.label("Content");}));});}
        for (point,expected) in [(rect.center(),false),(egui::pos2(10.,10.),true)]{
            let events=vec![egui::Event::PointerMoved(point),egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()},egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}];
            let _=ctx.run(input(events),|ctx|{assert_eq!(shell(ctx,"test-settings","Settings","Preferences",|ui,_|{ui.label("Content");}),expected);});
        }
    }
    #[test]fn switch_and_toggle_row_flip_on_click(){
        let ctx=egui::Context::default();let mut on=false;let mut rect=egui::Rect::NOTHING;
        let input=|events|egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,Vec2::new(600.,400.))),events,..Default::default()};
        for _ in 0..2{let _=ctx.run(input(vec![]),|ctx|{egui::CentralPanel::default().show(ctx,|ui|{rect=switch(ui,&mut on).rect;});});}
        let p=rect.center();let events=vec![egui::Event::PointerMoved(p),egui::Event::PointerButton{pos:p,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()},egui::Event::PointerButton{pos:p,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}];
        let _=ctx.run(input(events),|ctx|{egui::CentralPanel::default().show(ctx,|ui|{switch(ui,&mut on);});});
        assert!(on);
    }
    #[test]fn grey_theme_accents_fall_back_to_blurple(){
        let ctx=egui::Context::default();set_accent(&ctx,Color32::from_gray(184));assert_eq!(accent(&ctx),BLURPLE);
        set_accent(&ctx,Color32::from_rgb(65,183,205));assert_eq!(accent(&ctx),Color32::from_rgb(65,183,205));
    }
}
