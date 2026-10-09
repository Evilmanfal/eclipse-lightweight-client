use super::*;
use crate::presence::Status;
use crate::widgets::{paint_icon, HeaderIcon};
const WIDTH: f32 = 300.;
impl Eclipse {
    /// Opens or closes the Discord-style account menu above the bottom-left account bar.
    pub(in crate::ui) fn toggle_account_menu(&mut self,anchor:egui::Rect){
        if self.account_anchor.is_some(){if !self.account_just_opened{self.account_anchor=None;}return;}
        self.account_anchor=Some(anchor);self.account_just_opened=true;self.account_expanded=None;
        if !self.features.contains_key("my-profile"){self.load_my_profile();}
    }
    /// Sets your presence: locally, in Discord's saved settings and on the live gateway session.
    pub(in crate::ui) fn set_status(&mut self,status:&str){
        let Some(me)=self.user.as_ref().map(|u|u.id.clone()) else{return};
        self.presences.set(me,String::new(),Status::parse(status));
        if self.preview{return;}
        self.mutate("save-settings",reqwest::Method::PATCH,"/users/@me/settings".into(),Some(serde_json::json!({"status":status})));
        self.send_gateway(serde_json::json!({"op":3,"d":{"status":status,"since":0,"activities":[],"afk":false}}));
    }
    pub(in crate::ui) fn account_menu(&mut self,ctx:&egui::Context){
        let Some(anchor)=self.account_anchor else{return};
        let Some(me)=self.user.clone() else{self.account_anchor=None;return};
        let data=self.features.get("my-profile").cloned().unwrap_or(Value::Null);
        let user=crate::identity::merged(&me,&data);let p=super::profile_popout::profile_metadata(&data);
        let (primary,top,bottom)=self.profile_palette(&user,&p);
        let status=self.presences.get(&user.id,None);
        let mut open=true;let just_opened=std::mem::take(&mut self.account_just_opened);
        let mut action=None;
        egui::Popup::new(egui::Id::new("account-menu"),ctx.clone(),anchor,egui::LayerId::background())
            .open_bool(&mut open).align(egui::RectAlign::TOP_START).gap(8.).width(WIDTH)
            .close_behavior(if just_opened{egui::PopupCloseBehavior::IgnoreClicks}else{egui::PopupCloseBehavior::CloseOnClickOutside})
            .frame(egui::Frame::NONE.stroke(Stroke::new(1.0_f32,primary.gamma_multiply(0.45))).corner_radius(16))
            .show(|ui|{
                let start=ui.cursor().min;ui.set_width(WIDTH);ui.spacing_mut().item_spacing.y=6.;
                // The gradient is painted into a placeholder once the menu's height is known.
                let background=ui.painter().add(egui::Shape::Noop);
                let (banner,_)=ui.allocate_exact_size(Vec2::new(WIDTH,96.),egui::Sense::hover());
                ui.painter().rect_filled(banner,egui::CornerRadius{nw:16,ne:16,sw:0,se:0},primary);
                crate::identity::paint_art(ui,&mut self.images,banner,crate::identity::banner(&user,&data,None),16);
                crate::identity::vertical_gradient(ui.painter(),banner.with_min_y(banner.top()+30.).expand2(Vec2::new(0.,1.)),Color32::TRANSPARENT,top,0.);
                let portrait=egui::Rect::from_min_size(banner.left_bottom()+Vec2::new(16.,-46.),Vec2::splat(80.));
                ui.painter().circle_filled(portrait.center(),45.,top);
                let url=if self.preview{Some(format!("demo://user/{}",user.id))}else{assets::avatar_url(&user,None,None)};
                ui.painter().circle_filled(portrait.center(),40.,CARD);self.paint_image(ui,portrait,url,40);
                crate::identity::paint_art_playing(ui,&mut self.images,portrait.expand(8.),crate::identity::decoration(&user),0);
                crate::presence::paint_badge(ui.painter(),portrait,status);
                if ui.interact(portrait,egui::Id::new("account-menu-avatar"),egui::Sense::click()).on_hover_text("View full profile").clicked(){action=Some(Action::FullProfile);}
                ui.add_space(38.);
                egui::Frame::NONE.inner_margin(egui::Margin{left:14,right:14,top:0,bottom:12}).show(ui,|ui|{
                    ui.set_width(WIDTH-28.);ui.spacing_mut().item_spacing.y=4.;
                    ui.horizontal(|ui|{crate::identity::name(ui,&user,user.name(),20.,TEXT);self.guild_tag_chip(ui,&user);});
                    let pronouns=p["pronouns"].as_str().or(user.pronouns.as_deref()).filter(|s|!s.is_empty());
                    ui.label(RichText::new(match pronouns{Some(p)=>format!("{}  •  {p}",user.username),None=>user.username.clone()}).size(13.).color(Color32::from_gray(215)));
                    if let Some(bio)=p["bio"].as_str().or(user.bio.as_deref()).filter(|s|!s.is_empty()){ui.add(egui::Label::new(RichText::new(bio.lines().take(3).collect::<Vec<_>>().join("\n")).size(13.)).wrap());}
                    let badges:Vec<Value>=data["badges"].as_array().into_iter().flatten().take(16).cloned().collect();
                    if !badges.is_empty(){ui.horizontal_wrapped(|ui|{ui.spacing_mut().item_spacing=Vec2::new(4.,4.);for b in &badges{let(rect,response)=ui.allocate_exact_size(Vec2::splat(20.),egui::Sense::hover());crate::identity::paint_art(ui,&mut self.images,rect,b["icon"].as_str().and_then(crate::identity::badge),0);response.on_hover_text(b["description"].as_str().unwrap_or("Profile badge"));}});}
                    ui.add_space(6.);
                    group(ui,|ui|{
                        if row(ui,Icon::Glyph(HeaderIcon::Edit),"Edit Profile",false).clicked(){action=Some(Action::EditProfile);}
                        let label=match status{Status::Online=>"Online",Status::Idle=>"Idle",Status::Dnd=>"Do Not Disturb",Status::Offline=>"Invisible",Status::Unknown=>"Set status"};
                        if row(ui,Icon::Status(status),label,true).clicked(){self.account_expanded=if self.account_expanded==Some(0){None}else{Some(0)};}
                        if self.account_expanded==Some(0){
                            for (value,name,hint) in [("online","Online",""),("idle","Idle",""),("dnd","Do Not Disturb","You will not receive desktop notifications"),("invisible","Invisible","You will appear offline")]{
                                let response=sub_row(ui,Icon::Status(Status::parse(value)),name,hint);
                                if response.clicked(){action=Some(Action::Status(value));}
                            }
                        }
                    });
                    ui.add_space(6.);
                    group(ui,|ui|{
                        if row(ui,Icon::Glyph(HeaderIcon::Profile),"Switch Accounts",true).clicked(){self.account_expanded=if self.account_expanded==Some(1){None}else{Some(1)};}
                        if self.account_expanded==Some(1){
                            if sub_row(ui,Icon::Glyph(HeaderIcon::Profile),"Log out","Sign in with another account").clicked(){action=Some(Action::LogOut);}
                        }
                        if row(ui,Icon::Id,"Copy User ID",false).clicked(){action=Some(Action::CopyId);}
                    });
                });
                let rect=egui::Rect::from_min_max(start,egui::pos2(start.x+WIDTH,ui.min_rect().bottom()));
                ui.painter().set(background,crate::identity::vertical_gradient_shape(rect,top,bottom,16.));
            });
        if !open{self.account_anchor=None;}
        match action{
            Some(Action::EditProfile)=>{self.account_anchor=None;self.open_settings();self.settings_page="Account & Profile".into();self.load_settings_section();}
            Some(Action::Status(value))=>{self.set_status(value);self.account_expanded=None;}
            Some(Action::FullProfile)=>{self.account_anchor=None;self.open_full_profile(&me);}
            Some(Action::CopyId)=>{ctx.copy_text(me.id.clone());self.account_anchor=None;}
            Some(Action::LogOut)=>{self.account_anchor=None;self.log_out();}
            None=>{}
        }
    }
}
enum Action { EditProfile, Status(&'static str), FullProfile, CopyId, LogOut }
enum Icon { Glyph(HeaderIcon), Status(Status), Id }
/// A dark rounded card grouping menu rows, like Discord's account menu.
fn group(ui:&mut egui::Ui,content:impl FnOnce(&mut egui::Ui)){
    egui::Frame::NONE.fill(Color32::from_black_alpha(110)).corner_radius(10).inner_margin(4).show(ui,|ui|{ui.set_width(ui.available_width());ui.spacing_mut().item_spacing.y=2.;content(ui);});
}
fn row(ui:&mut egui::Ui,icon:Icon,label:&str,chevron:bool)->egui::Response{
    let (rect,response)=ui.allocate_exact_size(Vec2::new(ui.available_width(),36.),egui::Sense::click());
    if response.hovered(){ui.painter().rect_filled(rect,7,Color32::from_white_alpha(16));}
    paint_row_icon(ui,egui::pos2(rect.left()+18.,rect.center().y),&icon);
    ui.painter().text(egui::pos2(rect.left()+38.,rect.center().y),egui::Align2::LEFT_CENTER,label,egui::FontId::proportional(14.),TEXT);
    if chevron{let c=egui::pos2(rect.right()-14.,rect.center().y);let s=Stroke::new(1.6_f32,MUTED);ui.painter().line_segment([c+Vec2::new(-2.,-4.),c+Vec2::new(2.,0.)],s);ui.painter().line_segment([c+Vec2::new(2.,0.),c+Vec2::new(-2.,4.)],s);}
    response
}
fn sub_row(ui:&mut egui::Ui,icon:Icon,label:&str,hint:&str)->egui::Response{
    let height=if hint.is_empty(){30.}else{42.};
    let (rect,response)=ui.allocate_exact_size(Vec2::new(ui.available_width(),height),egui::Sense::click());
    if response.hovered(){ui.painter().rect_filled(rect,7,Color32::from_white_alpha(16));}
    paint_row_icon(ui,egui::pos2(rect.left()+30.,rect.top()+15.),&icon);
    ui.painter().text(egui::pos2(rect.left()+46.,rect.top()+15.),egui::Align2::LEFT_CENTER,label,egui::FontId::proportional(13.),TEXT);
    if !hint.is_empty(){ui.painter().text(egui::pos2(rect.left()+46.,rect.top()+31.),egui::Align2::LEFT_CENTER,hint,egui::FontId::proportional(11.),MUTED);}
    response
}
fn paint_row_icon(ui:&egui::Ui,center:egui::Pos2,icon:&Icon){
    match icon{
        Icon::Glyph(kind)=>paint_icon(ui.painter(),center,0.62,*kind,Stroke::new(1.6_f32,Color32::from_gray(205))),
        Icon::Status(status)=>{let rect=egui::Rect::from_center_size(center-Vec2::splat(3.),Vec2::splat(18.));crate::presence::paint_badge(ui.painter(),rect,*status);},
        Icon::Id=>{let rect=egui::Rect::from_center_size(center,Vec2::new(18.,13.));ui.painter().rect_stroke(rect,3,Stroke::new(1.4_f32,Color32::from_gray(205)),egui::StrokeKind::Inside);ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,"ID",egui::FontId::proportional(8.),Color32::from_gray(205));},
    }
}
