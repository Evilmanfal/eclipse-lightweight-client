use super::*;
impl Eclipse {
    pub(in crate::ui) fn extra_dialogs(&mut self,ctx:&egui::Context){
        self.settings_window(ctx);self.server_settings_window(ctx);
        self.profile_popout(ctx);self.full_profile_window(ctx);self.account_menu(ctx);crate::message_media::viewer(ctx,&mut self.images);crate::message_media::save_status(ctx);self.update_prompt(ctx);
        if let Some((title,command))=self.confirm.take(){let mut open=true;let mut decision=None;egui::Window::new("Confirm action").id(egui::Id::new("confirm-action")).open(&mut open).collapsible(false).resizable(false).show(ctx,|ui|{ui.label(&title);ui.horizontal(|ui|{if ui.button(RichText::new("Confirm").color(Color32::LIGHT_RED)).clicked(){decision=Some(true);}if ui.button("Cancel").clicked(){decision=Some(false);}});});match decision{Some(true)=>{if self.preview{self.error=Some("Offline preview: no action sent.".into());}else{self.send_command(command);}},Some(false)=>{},None if open=>self.confirm=Some((title,command)),None=>{}}}
    }
    pub(in crate::ui) fn updater_later(&self)->bool{self.updater.later()}
    /// The launch-time update prompt, the progress dialog while installing, and the green update
    /// button in the top right after the user chose No.
    pub(in crate::ui) fn update_prompt(&mut self,ctx:&egui::Context){
        use crate::updater::State;
        if let Some(exe)=self.updater.poll(){
            // Start the new version and close this one.
            if std::process::Command::new(&exe).spawn().is_ok(){ctx.send_viewport_cmd(egui::ViewportCommand::Close);}
            else{self.error=Some("Eclipse was updated. Restart it to use the new version.".into());}
        }
        if let State::Failed(_,error)=&self.updater.state{self.error=Some(format!("Update failed: {error}"));}
        let green=Color32::from_rgb(35,165,90);
        match &self.updater.state{
            State::Prompt(release)=>{
                let version=release.version.clone();let mut choice=None;
                let modal=egui::Modal::new(egui::Id::new("update-prompt")).show(ctx,|ui|{
                    ui.set_width(340.0);
                    ui.label(RichText::new("Update available").size(18.0).strong());ui.add_space(6.0);
                    ui.label(format!("Eclipse {version} is available. You have {}. Update now?",env!("CARGO_PKG_VERSION")));
                    ui.label(RichText::new("Eclipse will download the update from GitHub and restart.").size(12.0).color(MUTED));
                    ui.add_space(12.0);
                    ui.horizontal(|ui|{
                        if ui.add(egui::Button::new(RichText::new("Yes").color(Color32::WHITE).strong()).fill(green).min_size(Vec2::new(80.0,30.0))).clicked(){choice=Some(true);}
                        if ui.add(egui::Button::new("No").min_size(Vec2::new(80.0,30.0))).clicked(){choice=Some(false);}
                    });
                });
                if modal.should_close()&&choice.is_none(){choice=Some(false);}
                match choice{Some(true)=>self.updater.install(ctx),Some(false)=>self.updater.decline(),None=>{}}
            }
            State::Installing(release,_)=>{
                let version=release.version.clone();
                egui::Modal::new(egui::Id::new("update-progress")).show(ctx,|ui|{
                    ui.set_width(300.0);
                    ui.horizontal(|ui|{ui.spinner();ui.label(format!("Downloading Eclipse {version}…"));});
                    ui.label(RichText::new("Eclipse will restart when the update is installed.").size(12.0).color(MUTED));
                });
            }
            _=>{}
        }
        if let State::Failed(release,_)=&self.updater.state{self.updater.state=State::Later(release.clone());}
        if let State::Later(release)=&self.updater.state{
            let tip=format!("Update to Eclipse {}",release.version);
            let rect=egui::Rect::from_min_size(egui::pos2(ctx.screen_rect().right()-50.0,14.0),Vec2::splat(32.0));
            let clicked=egui::Area::new(egui::Id::new("update-button")).order(egui::Order::Foreground).fixed_pos(rect.min).show(ctx,|ui|{
                let (rect,response)=ui.allocate_exact_size(rect.size(),egui::Sense::click());
                let fill=if response.hovered(){Color32::from_rgb(45,190,105)}else{green};
                ui.painter().circle_filled(rect.center(),16.0,fill);
                // Download arrow.
                let (c,s)=(rect.center(),Stroke::new(2.0_f32,Color32::WHITE));
                ui.painter().line_segment([c-Vec2::new(0.0,7.0),c+Vec2::new(0.0,3.0)],s);
                ui.painter().line_segment([c+Vec2::new(-4.5,-1.5),c+Vec2::new(0.0,3.0)],s);
                ui.painter().line_segment([c+Vec2::new(4.5,-1.5),c+Vec2::new(0.0,3.0)],s);
                ui.painter().line_segment([c+Vec2::new(-6.0,7.0),c+Vec2::new(6.0,7.0)],s);
                response.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
            }).inner;
            if clicked{self.updater.reopen();}
        }
    }
    pub(in crate::ui) fn message_menu(&mut self,ui:&mut egui::Ui,message:&Message){
        if ui.button("Reply").clicked(){self.reply=Some(message.clone());ui.close();}
        if ui.button("Copy text").clicked(){ui.ctx().copy_text(message.content.clone());ui.close();}
        ui.menu_button("Add reaction",|ui|{for emoji in ["👍","❤️","😂","🎉","👀","✅"]{if ui.button(emoji).clicked(){self.react(message,emoji.into(),false);ui.close();}}});
        let mine=self.user.as_ref().is_some_and(|u|u.id==message.author.id);let actor=self.user.as_ref().map(|u|u.id.as_str()).unwrap_or("");let can_manage=self.guild.is_some()&&self.server.can(actor,13);
        if mine&&ui.button("Edit message").clicked(){self.start_edit(message);ui.close();}
        if (mine||can_manage)&&ui.button(RichText::new("Delete message").color(Color32::LIGHT_RED)).clicked(){self.delete=Some(message.id.clone());ui.close();}
        if (can_manage||self.channel.as_ref().is_some_and(|c|c.guild_id.is_none()))&&ui.button(if message.pinned{"Unpin message"}else{"Pin message"}).clicked(){self.mutate("pin-message",if message.pinned{Method::DELETE}else{Method::PUT},format!("/channels/{}/messages/pins/{}",message.channel_id,message.id),None);ui.close();}
        if ui.button("Mark unread").clicked(){let previous=message.id.parse::<u64>().ok().and_then(|n|n.checked_sub(1));if let Some(id)=previous{self.mutate("mark-unread",Method::POST,format!("/channels/{}/messages/{id}/ack",message.channel_id),Some(json!({"manual":true,"mention_count":1})));}ui.close();}
        if ui.button("Copy message link").clicked(){ui.ctx().copy_text(format!("https://discord.com/channels/{}/{}/{}",self.guild.as_deref().unwrap_or("@me"),message.channel_id,message.id));ui.close();}if ui.button("Copy message ID").clicked(){ui.ctx().copy_text(message.id.clone());ui.close();}
        ui.separator();ui.menu_button("Author",|ui|self.user_menu(ui,&message.author));
    }
    pub(in crate::ui) fn message_click(&mut self,response:&egui::Response,message:&Message){
        if !self.prefs.click_actions{return;}
        if response.double_clicked(){self.reply=Some(message.clone());}
        if response.clicked(){let modifiers=response.ctx.input(|i|i.modifiers);if modifiers.ctrl{response.ctx.copy_text(message.content.clone());}else if modifiers.alt&&self.user.as_ref().is_some_and(|u|u.id==message.author.id){self.start_edit(message);}}
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
            if crate::widgets::control_at(ui,button,id.with("edit"),crate::widgets::Control::Edit,"Edit message").clicked(){self.start_edit(message);}
        }
        let button=egui::Rect::from_min_size(egui::pos2(rect.right()-30.0,rect.top()+2.0),Vec2::splat(28.0));
        if crate::widgets::control_at(ui,button,id.with("delete"),crate::widgets::Control::Delete,"Delete message").clicked(){self.delete_now(&message.channel_id,&message.id);}
    }
    /// Edits happen in place inside the message, like Discord: Enter saves, Esc cancels.
    pub(in crate::ui) fn start_edit(&mut self,message:&Message){
        self.edit=Some((message.id.clone(),message.content.clone()));self.focus_edit=true;
    }
    /// Deletes without asking; the Shift quick-delete button uses this directly.
    pub(in crate::ui) fn delete_now(&mut self,channel:&str,id:&str){
        if self.preview{self.messages.retain(|m|m.id!=id);}
        else{self.send_command(Command::Delete{channel:channel.to_owned(),id:id.to_owned()});}
        if self.edit.as_ref().is_some_and(|(editing,_)|editing==id){self.edit=None;}
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
