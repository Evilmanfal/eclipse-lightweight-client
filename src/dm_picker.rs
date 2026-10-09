use super::*;
impl Eclipse {
    pub(in crate::ui) fn open_dm_picker(&mut self,anchor:egui::Rect){self.dm_modal=true;self.dm_anchor=Some(anchor);self.dm_just_opened=true;self.dm_search.clear();self.request_feature("friends","/users/@me/relationships".into());}
    pub(in crate::ui) fn message_person(&mut self,user:&User){
        if let Some(channel)=self.dms.iter().find(|c|c.kind==1&&c.recipients.iter().any(|u|u.id==user.id)).cloned(){self.guild=None;self.channels=self.dms.clone();self.select_channel(channel);}
        else if self.preview{let channel=Channel{id:format!("dm-{}",user.id),kind:1,recipients:vec![user.clone()],..Default::default()};self.dms.push(channel.clone());self.guild=None;self.channels=self.dms.clone();self.select_channel(channel);}
        else{self.send_command(Command::OpenDm(user.id.clone()));}
    }
    pub(in crate::ui) fn dm_picker(&mut self,ctx:&egui::Context)->Option<egui::Rect>{
        if !self.dm_modal{return None;}let mut open=true;
        let just_opened=std::mem::take(&mut self.dm_just_opened);
        let anchor=self.dm_anchor.unwrap_or_else(||egui::Rect::from_min_size(egui::pos2(250.,338.),Vec2::ZERO));
        let popup=egui::Popup::new(egui::Id::new("dm-name-search"),ctx.clone(),anchor,egui::LayerId::background())
        .open_bool(&mut open).align(egui::RectAlign::BOTTOM_START).gap(6.).width(356.)
        .close_behavior(if just_opened{egui::PopupCloseBehavior::IgnoreClicks}else{egui::PopupCloseBehavior::CloseOnClickOutside})
        .frame(egui::Frame::NONE.fill(SIDE).stroke(Stroke::new(1_f32,BORDER)).corner_radius(18).inner_margin(16))
        .show(|ui|{
            ui.set_width(324.);
            ui.label(RichText::new("Find your people").size(21.).strong());ui.weak("Search friends, direct messages, and loaded server members.");ui.add_space(10.);
            let response=ui.add(egui::TextEdit::singleline(&mut self.dm_search).hint_text("Search a name or @username").desired_width(f32::INFINITY).margin(Vec2::new(12.,10.)));
            if just_opened{response.request_focus();}
            self.feature_status(ui,"friends");ui.add_space(10.);
            let known=self.dms.iter().flat_map(|c|c.recipients.iter().cloned().map(|u|(u,None)))
                .chain(self.server.members.values().map(|m|(m.user.clone(),m.nick.clone())))
                .chain(self.messages.iter().map(|m|(m.author.clone(),None)));
            let matches=crate::people::search(&self.dm_search,self.user.as_ref().map(|u|u.id.as_str()).unwrap_or(""),&self.friends,known);
            if matches.is_empty(){ui.add_space(15.);ui.label("No matching people");ui.weak("Try their username or refresh Friends to load more people.");}
            egui::ScrollArea::vertical().id_salt("dm-search-results").max_height((ctx.screen_rect().height()-250.).clamp(100.,300.)).show(ui,|ui|{
                for (user,nick) in matches {
                    let(rect,response)=ui.allocate_exact_size(Vec2::new(ui.available_width(),56.),egui::Sense::click());
                    if response.hovered(){ui.painter().rect_filled(rect,8,CARD);}
                    let icon=egui::Rect::from_min_size(rect.min+Vec2::new(8.,10.),Vec2::splat(36.));
                    let url=if self.preview{Some(format!("demo://user/{}",user.id))}else{assets::avatar_url(&user,None,None)};
                    ui.painter().circle_filled(icon.center(),18.,CARD);self.paint_image(ui,icon,url,18);crate::presence::badge(ui,icon,self.presences.get(&user.id,None));
                    ui.painter().text(rect.min+Vec2::new(55.,12.),egui::Align2::LEFT_TOP,nick.as_deref().unwrap_or_else(||user.name()),egui::FontId::proportional(15.),TEXT);
                    ui.painter().text(rect.min+Vec2::new(55.,33.),egui::Align2::LEFT_TOP,format!("@{}",user.username),egui::FontId::proportional(11.),MUTED);
                    if response.clicked(){self.message_person(&user);self.dm_modal=false;}
                }
            });
        });if !open{self.dm_modal=false;self.dm_anchor=None;}
        popup.map(|p|p.response.rect)
    }
}
