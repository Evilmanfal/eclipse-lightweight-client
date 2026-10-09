use super::*;
impl Eclipse {
    pub(in crate::ui) fn extra_dialogs(&mut self,ctx:&egui::Context){
        self.settings_window(ctx);self.server_settings_window(ctx);
        self.profile_popout(ctx);self.full_profile_window(ctx);self.account_menu(ctx);
        if let Some((title,command))=self.confirm.take(){let mut open=true;let mut decision=None;egui::Window::new("Confirm action").id(egui::Id::new("confirm-action")).open(&mut open).collapsible(false).resizable(false).show(ctx,|ui|{ui.label(&title);ui.horizontal(|ui|{if ui.button(RichText::new("Confirm").color(Color32::LIGHT_RED)).clicked(){decision=Some(true);}if ui.button("Cancel").clicked(){decision=Some(false);}});});match decision{Some(true)=>{if self.preview{self.error=Some("Offline preview: no action sent.".into());}else{self.send_command(command);}},Some(false)=>{},None if open=>self.confirm=Some((title,command)),None=>{}}}
    }
    pub(in crate::ui) fn message_menu(&mut self,ui:&mut egui::Ui,message:&Message){
        if ui.button("Reply").clicked(){self.reply=Some(message.clone());ui.close();}
        if ui.button("Copy text").clicked(){ui.ctx().copy_text(message.content.clone());ui.close();}
        ui.menu_button("Add reaction",|ui|{for emoji in ["👍","❤️","😂","🎉","👀","✅"]{if ui.button(emoji).clicked(){self.react(message,emoji.into(),false);ui.close();}}});
        let mine=self.user.as_ref().is_some_and(|u|u.id==message.author.id);let actor=self.user.as_ref().map(|u|u.id.as_str()).unwrap_or("");let can_manage=self.guild.is_some()&&self.server.can(actor,13);
        if mine&&ui.button("Edit message").clicked(){self.edit=Some((message.id.clone(),message.content.clone()));ui.close();}
        if (mine||can_manage)&&ui.button(RichText::new("Delete message").color(Color32::LIGHT_RED)).clicked(){self.delete=Some(message.id.clone());ui.close();}
        if (can_manage||self.channel.as_ref().is_some_and(|c|c.guild_id.is_none()))&&ui.button(if message.pinned{"Unpin message"}else{"Pin message"}).clicked(){self.mutate("pin-message",if message.pinned{Method::DELETE}else{Method::PUT},format!("/channels/{}/messages/pins/{}",message.channel_id,message.id),None);ui.close();}
        if ui.button("Mark unread").clicked(){let previous=message.id.parse::<u64>().ok().and_then(|n|n.checked_sub(1));if let Some(id)=previous{self.mutate("mark-unread",Method::POST,format!("/channels/{}/messages/{id}/ack",message.channel_id),Some(json!({"manual":true,"mention_count":1})));}ui.close();}
        if ui.button("Copy message link").clicked(){ui.ctx().copy_text(format!("https://discord.com/channels/{}/{}/{}",self.guild.as_deref().unwrap_or("@me"),message.channel_id,message.id));ui.close();}if ui.button("Copy message ID").clicked(){ui.ctx().copy_text(message.id.clone());ui.close();}
        ui.separator();ui.menu_button("Author",|ui|self.user_menu(ui,&message.author));
    }
    pub(in crate::ui) fn message_click(&mut self,response:&egui::Response,message:&Message){
        if !self.prefs.click_actions{return;}
        if response.double_clicked(){self.reply=Some(message.clone());}
        if response.clicked(){let modifiers=response.ctx.input(|i|i.modifiers);if modifiers.ctrl{response.ctx.copy_text(message.content.clone());}else if modifiers.alt&&self.user.as_ref().is_some_and(|u|u.id==message.author.id){self.edit=Some((message.id.clone(),message.content.clone()));}}
    }
    pub(in crate::ui) fn quick_message_actions(&mut self,ui:&mut egui::Ui,response:&egui::Response,message:&Message){
        if !ui.input(|i|i.modifiers.shift)||!ui.rect_contains_pointer(response.rect){return;}
        let mine=self.user.as_ref().is_some_and(|u|u.id==message.author.id);
        let actor=self.user.as_ref().map(|u|u.id.as_str()).unwrap_or("");
        let can_delete=mine||(self.guild.is_some()&&self.server.can(actor,13));
        if !mine&&!can_delete{return;}
        let count=if mine{2.0}else{1.0};
        let rect=egui::Rect::from_min_size(egui::pos2(response.rect.right()-count*32.0-8.0,response.rect.top()+2.0),Vec2::new(count*32.0+4.0,32.0));
        ui.painter().rect_filled(rect,10,CARD);
        ui.painter().rect_stroke(rect,10,Stroke::new(1_f32,BORDER),egui::StrokeKind::Inside);
        let id=egui::Id::new(("message-actions",&message.channel_id,&message.id));
        if mine{
            let button=egui::Rect::from_min_size(rect.min+Vec2::splat(2.0),Vec2::splat(28.0));
            if crate::widgets::control_at(ui,button,id.with("edit"),crate::widgets::Control::Edit,"Edit message").clicked(){self.edit=Some((message.id.clone(),message.content.clone()));}
        }
        let button=egui::Rect::from_min_size(egui::pos2(rect.right()-30.0,rect.top()+2.0),Vec2::splat(28.0));
        if crate::widgets::control_at(ui,button,id.with("delete"),crate::widgets::Control::Delete,"Delete message").clicked(){self.delete=Some(message.id.clone());}
    }
    pub(in crate::ui) fn channel_menu(&mut self,ui:&mut egui::Ui,channel:&Channel){
        if let Some(last)=self.read_latest.get(&channel.id).cloned(){if ui.button("Mark as read").clicked(){self.mutate("channel-read",Method::POST,format!("/channels/{}/messages/{last}/ack",channel.id),Some(json!({"token":null})));ui.close();}}
        if channel.guild_id.is_some(){if ui.button("Channel settings").clicked(){self.server_settings=true;self.server_page="Channels".into();ui.close();}}else if let Some(user)=channel.recipients.first(){self.user_menu(ui,user);ui.separator();}
        if ui.button("Copy channel ID").clicked(){ui.ctx().copy_text(channel.id.clone());ui.close();}if ui.button("Copy channel link").clicked(){ui.ctx().copy_text(format!("https://discord.com/channels/{}/{}",channel.guild_id.as_deref().unwrap_or("@me"),channel.id));ui.close();}
    }
    pub(in crate::ui) fn preferences_tick(&mut self,ctx:&egui::Context){
        if self.prefs!=self.applied_prefs{self.prefs.normalize();self.prefs.apply(ctx);self.calls.configure(&self.prefs);self.compact=self.prefs.compact;self.show_members=self.prefs.members;self.images.playback_options(self.prefs.animations&&!self.prefs.reduced_motion,self.prefs.animation_fps);self.applied_prefs=self.prefs.clone();self.prefs_save_at=Some(Instant::now()+Duration::from_millis(700));}
        if self.prefs_save_at.is_some_and(|at|Instant::now()>=at){self.prefs_save_at=None;if let Err(error)=self.prefs.save(){self.error=Some(error);}}
        if self.prefs.spotify&&!self.preview&&self.user.is_some(){if self.spotify.is_none(){self.spotify=Some(crate::spotify::Spotify::new(ctx.clone()));}if let Some(spotify)=&mut self.spotify{spotify.poll();}}else{self.spotify=None;}
        if self.prefs.push_to_talk&&self.calls.active(){ctx.request_repaint_after(Duration::from_millis(20));}
        if !self.prefs.message_logger{self.logs.clear();}
    }
}
