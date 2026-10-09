use super::*;
impl Eclipse {
    pub(in crate::ui) fn home_panel(&mut self,ctx:&egui::Context){
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(preferences::color(&self.prefs.theme.background).unwrap_or(BG)).inner_margin(egui::Margin{left:20,right:20,top:if self.updater_later(){56}else{20},bottom:20})).show(ctx,|ui|{
            // Friends lays out its own scrolling list beside the Active Now column.
            if self.home==Home::Friends{self.friends_content(ui);return;}
            egui::ScrollArea::vertical().id_salt("home-scroll").auto_shrink([false,false]).show(ui,|ui|{
                if self.preview{ui.weak("Offline preview · sample friends, no account data or purchases");ui.add_space(10.);}
                match self.home{Home::Nitro=>self.nitro_content(ui),Home::Shop=>self.shop_content(ui),Home::Quests=>self.quests_content(ui),Home::Friends|Home::Chat=>{}}
            });
        });
    }
    fn friends_content(&mut self,ui:&mut egui::Ui){
        ui.horizontal_wrapped(|ui|{ui.heading("Friends");for filter in ["Online","All","Pending","Blocked","Add Friend"]{ui.selectable_value(&mut self.friend_filter,filter.into(),filter);}if ui.button("Refresh").clicked(){self.request_feature("friends","/users/@me/relationships".into());}});ui.add_space(12.);self.feature_status(ui,"friends");self.feature_status(ui,"relationship");
        if self.friend_filter=="Add Friend"{heading(ui,"Add Friend","Enter a Discord username or a user ID.");ui.add(egui::TextEdit::singleline(&mut self.friend_add).hint_text("username").desired_width(420.));if ui.add_enabled(!self.friend_add.trim().is_empty(),primary("Send friend request")).clicked(){let value=self.friend_add.trim().to_owned();if community::snowflake(&value){self.mutate("relationship",Method::PUT,format!("/users/@me/relationships/{value}"),Some(json!({"type":1})));}else{self.mutate("relationship",Method::POST,"/users/@me/relationships".into(),Some(json!({"username":value,"discriminator":null})));}}return;}
        let active_width=if ui.available_width()>820.{300.}else{0.};
        let height=ui.available_height();
        ui.horizontal_top(|ui|{
            let list_width=ui.available_width()-if active_width>0.{active_width+24.}else{0.};
            ui.allocate_ui_with_layout(Vec2::new(list_width,height),egui::Layout::top_down(egui::Align::Min),|ui|{
                ui.set_width(list_width);
                ui.scope(|ui|{ui.visuals_mut().extreme_bg_color=CARD;ui.add(egui::TextEdit::singleline(&mut self.friend_search).hint_text("Search").desired_width(f32::INFINITY).margin(Vec2::new(10.,7.)));});
                ui.add_space(10.);
                let search=self.friend_search.to_lowercase();
                let mut friends:Vec<_>=self.friends.iter().filter(|r|{let filter=match self.friend_filter.as_str(){"Online"=>r.kind==1&&!matches!(self.presences.get(&r.id,None),crate::presence::Status::Offline|crate::presence::Status::Unknown),"Pending"=>matches!(r.kind,3|4),"Blocked"=>r.kind==2,_=>r.kind==1};filter&&(r.user.name().to_lowercase().contains(&search)||r.user.username.to_lowercase().contains(&search))}).cloned().collect();
                friends.sort_by_key(|r|r.user.name().to_lowercase());
                ui.label(RichText::new(format!("{} — {}",self.friend_filter,friends.len())).size(12.).strong().color(MUTED));ui.add_space(4.);
                egui::ScrollArea::vertical().id_salt("friends-list").auto_shrink([false,false]).show(ui,|ui|{
                    ui.spacing_mut().item_spacing.y=0.;
                    for friend in friends{self.friend_row(ui,&friend);}
                    if self.friends.is_empty()&&!self.feature_pending.contains("friends"){ui.add_space(12.);ui.weak("No friends loaded. Refresh to retrieve your Discord relationships.");}
                });
            });
            if active_width>0.{
                let x=ui.cursor().left()+11.;ui.painter().vline(x,ui.cursor().top()..=ui.cursor().top()+height,Stroke::new(1.0_f32,BORDER));ui.add_space(22.);
                ui.allocate_ui_with_layout(Vec2::new(active_width,height),egui::Layout::top_down(egui::Align::Min),|ui|{ui.set_width(active_width);self.active_now(ui);});
            }
        });
    }
    /// One compact friend row: avatar, name, status or activity, and round message/profile icons.
    fn friend_row(&mut self,ui:&mut egui::Ui,friend:&Relationship){
        let bg=ui.painter().add(egui::Shape::Noop);
        let inner=egui::Frame::NONE.inner_margin(egui::Margin::symmetric(8,6)).show(ui,|ui|{
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui|{
                self.user_avatar(ui,&friend.user,None,32.);ui.add_space(4.);
                ui.vertical(|ui|{ui.spacing_mut().item_spacing.y=1.;
                    ui.horizontal(|ui|{ui.label(RichText::new(friend.nickname.as_deref().unwrap_or_else(||friend.user.name())).size(14.).strong());self.guild_tag_chip(ui,&friend.user);});
                    let subtitle=match friend.kind{3=>"Incoming friend request".to_owned(),4=>"Outgoing friend request".to_owned(),2=>"Blocked".to_owned(),_=>self.presences.activity(&friend.id).map(|a|format!("{} {}",a.verb(),a.name)).unwrap_or_else(||self.presences.get(&friend.id,None).label().to_owned())};
                    ui.add(egui::Label::new(RichText::new(subtitle).size(12.).color(MUTED)).truncate());
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{ui.spacing_mut().item_spacing.x=8.;
                    match friend.kind{
                        3=>{if ui.small_button("Accept").clicked(){self.mutate("relationship",Method::PUT,format!("/users/@me/relationships/{}",friend.id),Some(json!({"type":1})));}if ui.small_button("Ignore").clicked(){self.mutate("relationship",Method::DELETE,format!("/users/@me/relationships/{}",friend.id),None);}},
                        4=>{if ui.small_button("Cancel request").clicked(){self.mutate("relationship",Method::DELETE,format!("/users/@me/relationships/{}",friend.id),None);}},
                        2=>{if ui.small_button("Unblock").clicked(){self.mutate("relationship",Method::DELETE,format!("/users/@me/relationships/{}",friend.id),None);}},
                        _=>{
                            let profile=crate::widgets::round_icon(ui,crate::widgets::HeaderIcon::Profile,"Profile");if profile.clicked(){self.toggle_profile_at(&friend.user,profile.rect);}
                            if crate::widgets::round_icon(ui,crate::widgets::HeaderIcon::Message,"Message").clicked(){self.message_person(&friend.user);}
                        },
                    }
                });
            });
        }).response;
        let response=ui.interact(inner.rect,egui::Id::new(("friend-row",&friend.id)),egui::Sense::click());
        if response.hovered(){ui.painter().set(bg,egui::Shape::rect_filled(inner.rect,8,Color32::from_gray(30)));}
        else{ui.painter().set(bg,egui::Shape::hline(inner.rect.x_range().shrink(8.),inner.rect.top(),Stroke::new(1.0_f32,Color32::from_gray(34))));}
        response.context_menu(|ui|self.user_menu(ui,&friend.user));
        if response.clicked()&&friend.kind==1{self.message_person(&friend.user);}
    }
    /// Friends' current activities grouped by what they are doing, like Discord's Active Now.
    fn active_now(&mut self,ui:&mut egui::Ui){
        ui.label(RichText::new("Active Now").size(18.).strong());ui.add_space(10.);
        let mut groups:Vec<(crate::presence::Activity,Vec<User>)>=vec![];
        for friend in self.friends.iter().filter(|r|r.kind==1){if let Some(activity)=self.presences.activity(&friend.id){
            match groups.iter_mut().find(|(a,_)|a.name==activity.name){Some((_,users))=>users.push(friend.user.clone()),None=>groups.push((activity.clone(),vec![friend.user.clone()]))}
        }}
        groups.sort_by_key(|(a,users)|(std::cmp::Reverse(users.len()),a.name.to_lowercase()));
        if groups.is_empty(){
            egui::Frame::NONE.fill(CARD).corner_radius(10).inner_margin(16).show(ui,|ui|{ui.set_min_width(ui.available_width());ui.vertical_centered(|ui|{ui.label(RichText::new("It's quiet for now...").strong());ui.label(RichText::new("When a friend starts an activity, like playing a game, it shows up here.").size(12.).color(MUTED));});});
            return;
        }
        let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d|d.as_millis() as u64).unwrap_or(0);
        egui::ScrollArea::vertical().id_salt("active-now").auto_shrink([false,false]).show(ui,|ui|{
            for (activity,users) in groups.into_iter().take(30){
                egui::Frame::NONE.fill(Color32::from_gray(24)).corner_radius(10).inner_margin(egui::Margin::symmetric(10,8)).show(ui,|ui|{
                    ui.set_min_width(ui.available_width());ui.spacing_mut().item_spacing.y=4.;
                    for user in users.iter().take(6){
                        let row=ui.horizontal(|ui|{
                            self.user_avatar(ui,user,None,30.);
                            ui.vertical(|ui|{ui.spacing_mut().item_spacing.y=0.;ui.label(RichText::new(user.name()).size(14.).strong());
                                let started=self.presences.activity(&user.id).and_then(|a|a.started_ms).filter(|s|*s<=now).map(|s|format!(" – {}",elapsed(now-s))).unwrap_or_default();
                                ui.label(RichText::new(format!("{}{started}",activity.name)).size(12.).color(MUTED));});
                        }).response;
                        if ui.interact(row.rect,egui::Id::new(("active-user",&user.id,&activity.name)),egui::Sense::click()).clicked(){self.toggle_profile_at(user,row.rect);}
                    }
                    ui.add_space(2.);ui.separator();
                    ui.horizontal(|ui|{
                        let(rect,_)=ui.allocate_exact_size(Vec2::splat(30.),egui::Sense::hover());
                        ui.painter().rect_filled(rect,7,CARD);ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,initials_of(&activity.name),egui::FontId::proportional(10.),TEXT);
                        crate::identity::paint_art(ui,&mut self.images,rect,activity.image.clone(),7);
                        ui.vertical(|ui|{ui.spacing_mut().item_spacing.y=0.;ui.label(RichText::new(&activity.name).size(13.).strong());ui.label(RichText::new(if users.len()==1{"1 Person".to_owned()}else{format!("{} People",users.len())}).size(11.).color(MUTED));});
                    });
                });
                ui.add_space(10.);
            }
        });
    }
    pub(in crate::ui) fn nitro_content(&mut self,ui:&mut egui::Ui){
        egui::Frame::NONE.fill(Color32::from_rgb(72,48,114)).corner_radius(16).inner_margin(26).show(ui,|ui|{
            let width=ui.available_width();ui.set_min_width(width);ui.set_min_height(170.);
            let rect=egui::Rect::from_min_size(ui.cursor().min,Vec2::new(width,170.));crate::identity::gradient(ui,rect.expand(26.),Color32::from_rgb(74,38,129),Color32::from_rgb(167,70,167));
            let show_art=width>480.;if show_art{let art=egui::Rect::from_center_size(egui::pos2(rect.right()-105.,rect.center().y),Vec2::splat(210.));crate::identity::paint_art(ui,&mut self.images,art,Some("builtin://discord/nitro-wumpus".into()),0);}
            ui.scope(|ui|{ui.set_max_width(if show_art{width-225.}else{width});ui.label(RichText::new("NITRO").size(38.).strong().color(Color32::WHITE));ui.label(RichText::new("A little more you.").size(22.).color(Color32::WHITE));ui.add_space(12.);ui.label(nitro_name(self.user.as_ref().map(|u|u.premium_type).unwrap_or(0)));ui.hyperlink_to("Explore or manage Discord Nitro","https://discord.com/nitro");});
        });ui.add_space(20.);
        for(title,description)in[("Express yourself","Custom and animated emoji, animated avatars and profile customization where your subscription permits."),("Bigger uploads and longer messages","Nitro message length is enabled up to 4,000 characters; account upload limits are enforced by Discord."),("Better streaming","Native 1080p / 60fps options for Nitro or eligible boosted servers. This renderer does not yet support 4K streaming."),("Boost your communities","View your current boost slots in User Settings → Server Boost."),("Shop and rewards","Browse Discord's live collectibles catalog and view the quests offered to your account.")]{egui::Frame::group(ui.style()).corner_radius(10).inner_margin(18).show(ui,|ui|{ui.strong(title);ui.label(description);});ui.add_space(10.);}
        ui.weak("Discord determines your paid entitlements. This client does not grant Nitro, unlock purchased items or process payments.");
    }
    pub(in crate::ui) fn spotify_card(&mut self,ui:&mut egui::Ui){
        let Some(spotify)=&self.spotify else{return;};let track=spotify.track.clone();
        if !track.available{return;}
        egui::Frame::NONE.fill(Color32::from_rgb(24,39,32)).corner_radius(10).inner_margin(12).show(ui,|ui|{
            ui.set_min_width(ui.available_width());ui.label(RichText::new("SPOTIFY").size(10.).color(Color32::from_rgb(30,215,96)));ui.add(egui::Label::new(RichText::new(&track.title).strong()).truncate());ui.add(egui::Label::new(RichText::new(&track.artist).small().weak()).truncate());
            let mut position=track.position;let response=ui.add(egui::Slider::new(&mut position,0.0..=track.duration.max(1.)).show_value(false));if response.drag_stopped(){spotify.send(crate::spotify::Action::Seek(position));}
            ui.horizontal(|ui|{if ui.selectable_label(track.shuffle,"⇄").on_hover_text("Shuffle").clicked(){spotify.send(crate::spotify::Action::Shuffle(!track.shuffle));}if ui.small_button("⏮").on_hover_text("Previous track").clicked(){spotify.send(crate::spotify::Action::Previous);}if ui.button(if track.playing{"Pause"}else{"Play"}).clicked(){spotify.send(crate::spotify::Action::Toggle);}if ui.small_button("⏭").on_hover_text("Next track").clicked(){spotify.send(crate::spotify::Action::Next);}if ui.selectable_label(track.repeat,"↻").on_hover_text("Repeat track").clicked(){spotify.send(crate::spotify::Action::Repeat(!track.repeat));}ui.label(RichText::new(format!("{}:{:02}",position as u64/60,position as u64%60)).small());});if let Some(error)=&track.error{ui.weak(error);}
        });ui.add_space(8.);
    }
}
/// Compact elapsed time for activities: "45m", "4h", "2d".
fn elapsed(ms:u64)->String{let minutes=ms/60_000;if minutes<60{format!("{}m",minutes.max(1))}else if minutes<60*24{format!("{}h",minutes/60)}else{format!("{}d",minutes/(60*24))}}
