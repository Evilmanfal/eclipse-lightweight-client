use super::*;
impl Eclipse {
    pub(in crate::ui) fn profile_key(&self,id:&str)->String {profile_key_for(id,self.profile_guild.as_deref())}
    pub(in crate::ui) fn show_profile_at(&mut self,user:&User,anchor:egui::Rect) {
        self.profile=Some(user.clone());self.profile_anchor=Some(anchor);self.profile_guild=self.guild.clone();self.profile_just_opened=true;
        let key=self.profile_key(&user.id);
        self.request_feature(&key,profile_route(&user.id,self.profile_guild.as_deref()));
    }
    /// Profile colours: Nitro theme colours as a gradient, otherwise the accent fading into the surface.
    /// Returns (banner colour, gradient top, gradient bottom).
    pub(in crate::ui) fn profile_palette(&self,user:&User,p:&Value)->(Color32,Color32,Color32){
        let primary=p["theme_colors"][0].as_u64().or_else(||p["accent_color"].as_u64()).or(user.accent_color.map(u64::from)).map(|c|crate::identity::color(c as u32)).unwrap_or(self.accent());
        match p["theme_colors"][1].as_u64().map(|c|crate::identity::color(c as u32)){
            Some(secondary)=>(primary,primary.lerp_to_gamma(Color32::BLACK,0.35),secondary.lerp_to_gamma(Color32::BLACK,0.5)),
            None=>(primary,primary.lerp_to_gamma(SIDE,0.72),SIDE),
        }
    }
    /// Direct clicks on a user toggle: clicking the same user again closes their open profile.
    pub(in crate::ui) fn toggle_profile_at(&mut self,user:&User,anchor:egui::Rect) {
        if self.profile.as_ref().is_some_and(|u|u.id==user.id)&&self.profile_guild==self.guild{
            // Nested hit regions (avatar, name, row) can report the same click; keep a profile opened this frame.
            if !self.profile_just_opened{self.profile=None;self.profile_anchor=None;}
            return;
        }
        self.show_profile_at(user,anchor);
    }
    pub(in crate::ui) fn profile_popout(&mut self,ctx:&egui::Context)->Option<egui::Rect> {
        let Some(user)=self.profile.clone() else{return None;};
        let anchor=self.profile_anchor.unwrap_or_else(||egui::Rect::from_min_size(ctx.screen_rect().center(),Vec2::ZERO));
        let key=self.profile_key(&user.id);
        let data=self.features.get(&key).cloned().unwrap_or(Value::Null);
        let user=crate::identity::merged(&user,&data);
        let p=profile_metadata(&data);
        let effect_id=p["profile_effect"]["id"].as_str().map(str::to_owned);
        if effect_id.is_some()&&!self.features.contains_key("profile-effects")&&!self.feature_errors.contains_key("profile-effects"){self.request_feature("profile-effects","/user-profile-effects".into());}
        let (primary_color,top,bottom)=self.profile_palette(&user,&p);
        let mut open=true;let mut close=false;let mut full=false;let mut portrait_marker=None;
        let just_opened=std::mem::take(&mut self.profile_just_opened);
        let popout=egui::Popup::new(egui::Id::new("native-profile-popout"),ctx.clone(),anchor,egui::LayerId::background())
            .open_bool(&mut open).align(egui::RectAlign::RIGHT_START).gap(10.).width(330.)
            .close_behavior(if just_opened{egui::PopupCloseBehavior::IgnoreClicks}else{egui::PopupCloseBehavior::CloseOnClickOutside})
            .frame(egui::Frame::NONE.stroke(Stroke::new(1.0_f32,primary_color.gamma_multiply(0.55))).corner_radius(18).inner_margin(8))
            .show(|ui|{
                let height=(ctx.screen_rect().height()-90.).clamp(260.,560.);
                let frame=egui::Rect::from_min_size(ui.cursor().min-Vec2::splat(8.),Vec2::new(330.,height+16.));
                crate::identity::vertical_gradient(ui.painter(),frame,top,bottom,18.);
                ui.set_width(314.);ui.set_height(height);ui.spacing_mut().item_spacing.y=6.;
                egui::ScrollArea::vertical().id_salt("profile-content").max_height(height).auto_shrink([false,false]).show(ui,|ui|{
                    let (banner,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),94.),egui::Sense::hover());
                    ui.painter().rect_filled(banner,10,primary_color);
                    crate::identity::paint_art(ui,&mut self.images,banner,crate::identity::banner(&user,&data,self.profile_guild.as_deref()),10);
                    let fade_to=top.lerp_to_gamma(bottom,((banner.bottom()-frame.top())/frame.height()).clamp(0.,1.));
                    crate::identity::vertical_gradient(ui.painter(),banner.with_min_y(banner.center().y-10.).expand2(Vec2::new(0.,1.)),Color32::TRANSPARENT,fade_to,0.);
                    ui.add_space(4.);
                    ui.horizontal_top(|ui|{
                        let member=data["guild_member"]["avatar"].as_str();
                        let portrait=avatar(ui,user.name(),66.);portrait_marker=Some(portrait);
                        if ui.interact(portrait,egui::Id::new("profile-popout-avatar"),egui::Sense::click()).on_hover_text("View full profile").clicked(){full=true;}let url=if self.preview{Some(format!("demo://user/{}",user.id))}else{assets::avatar_url(&user,self.profile_guild.as_deref(),member)};self.paint_image(ui,portrait,url,33);
                        let mut decorated=user.clone();if let Some(value)=data["guild_member"]["avatar_decoration_data"].as_object(){decorated.avatar_decoration_data=Some(Value::Object(value.clone()));}crate::identity::paint_art_playing(ui,&mut self.images,portrait.expand(6.6),crate::identity::decoration(&decorated),0);crate::presence::badge(ui,portrait,self.presences.get(&user.id,self.profile_guild.as_deref()));
                        ui.allocate_ui_with_layout(Vec2::new(ui.available_width(),66.),egui::Layout::right_to_left(egui::Align::Min),|ui|{
                            ui.menu_button("•••",|ui|self.user_menu(ui,&user));
                        });
                    });
                    ui.add_space(4.);
                    let nick=data["guild_member"]["nick"].as_str().or_else(||self.server.members.get(&user.id).filter(|_|self.profile_guild.as_deref()==Some(&self.server.id)).and_then(|m|m.nick.as_deref())).unwrap_or_else(||user.name()).to_owned();
                    let name_rect=egui::Rect::from_min_size(ui.cursor().min,Vec2::new(ui.available_width(),34.));
                    crate::identity::paint_art_playing(ui,&mut self.images,name_rect,crate::identity::nameplate(&user),7);
                    ui.horizontal(|ui|{crate::identity::name(ui,&user,&nick,23.,TEXT);self.guild_tag_chip(ui,&user);});
                    ui.label(RichText::new(format!("@{}",user.username)).size(12.).color(MUTED));
                    if data["badges"].as_array().is_some_and(|a|!a.is_empty())||data["guild_badges"].as_array().is_some_and(|a|!a.is_empty()){ui.horizontal_wrapped(|ui|{ui.spacing_mut().item_spacing=Vec2::new(5.,4.);for b in data["badges"].as_array().into_iter().flatten().chain(data["guild_badges"].as_array().into_iter().flatten()).take(24){let(rect,response)=ui.allocate_exact_size(Vec2::splat(20.),egui::Sense::hover());crate::identity::paint_art(ui,&mut self.images,rect,b["icon"].as_str().and_then(crate::identity::badge),0);response.on_hover_text(b["description"].as_str().unwrap_or("Profile badge"));}});}
                    if let Some(pronouns)=p["pronouns"].as_str().or(user.pronouns.as_deref()).filter(|s|!s.is_empty()){ui.label(RichText::new(pronouns).size(12.).color(MUTED));}
                    let status=self.presences.get(&user.id,self.profile_guild.as_deref());ui.label(RichText::new(status.label()).size(12.).color(MUTED));
                    ui.add_space(9.);ui.separator();
                    if let Some(bio)=p["bio"].as_str().or(user.bio.as_deref()).filter(|s|!s.is_empty()) {ui.label(RichText::new("ABOUT ME").size(10.).strong().color(MUTED));crate::message_media::body(ui,&mut self.images,bio);ui.add_space(8.);}
                    if let Some(member)=self.server.members.get(&user.id).cloned().filter(|_|self.profile_guild.as_deref()==Some(&self.server.id)){
                        let roles:Vec<_>=self.server.roles.iter().filter(|r|member.roles.contains(&r.id)).cloned().collect();
                        if !roles.is_empty(){ui.label(RichText::new("ROLES").size(10.).strong().color(MUTED));ui.horizontal_wrapped(|ui|{for role in roles{egui::Frame::NONE.fill(Color32::from_black_alpha(55)).corner_radius(5).inner_margin(egui::Margin::symmetric(7,4)).show(ui,|ui|{ui.horizontal(|ui|{let(rect,_)=ui.allocate_exact_size(Vec2::splat(8.),egui::Sense::hover());ui.painter().circle_filled(rect.center(),4.,if role.rgb()==0{MUTED}else{crate::identity::color(role.rgb())});ui.label(RichText::new(&role.name).size(11.));});});}});ui.add_space(8.);}
                    }
                    self.feature_status(ui,&key);
                    ui.add_space(8.);
                    if self.user.as_ref().is_some_and(|u|u.id==user.id){if ui.add_sized([ui.available_width(),32.],primary("Edit profile")).clicked(){self.open_settings();self.settings_page="Account & Profile".into();close=true;}}
                    else{ui.horizontal(|ui|{if ui.add_sized([ui.available_width()-66.,32.],primary("Message")).clicked(){self.message_person(&user);close=true;}if ui.add_sized([56.,32.],egui::Button::new("Call")).clicked(){if let Some(dm)=self.dms.iter().find(|c|c.kind==1&&c.recipients.iter().any(|u|u.id==user.id)).cloned(){if self.preview{self.calls.preview(dm,self.user.clone().unwrap_or_default(),vec![user.clone()]);}else{self.start_call(&dm,false);}close=true;}else{self.error=Some("Open a direct message first to call this person.".into());}}});}
                    if self.preview{ui.add_space(4.);ui.weak("Sample profile · offline preview");}
                });
            });
        if let Some(response)=&popout {if let Some(id)=effect_id{
            let configs=self.features.get("profile-effects").cloned().unwrap_or(Value::Null);
            if let Some(effect)=configs["profile_effect_configs"].as_array().or_else(||configs["profile_effects"].as_array()).into_iter().flatten().find(|e|e["id"].as_str()==Some(&id)){
                let config=effect.get("config").unwrap_or(effect);let moving=self.prefs.animations&&!self.prefs.reduced_motion;
                let url=if moving{config["thumbnailPreviewSrc"].as_str().or_else(||config["reducedMotionSrc"].as_str())}else{config["staticFrameSrc"].as_str().or_else(||config["reducedMotionSrc"].as_str())};
                if let Some(url)=url.filter(|u|assets::public_url(u)){if let Some(texture)=self.images.texture_hover(url,response.response.rect,ctx){let painter=ctx.layer_painter(response.response.layer_id).with_clip_rect(response.response.rect);let rect=response.response.rect;let mesh=egui::Mesh::with_texture(texture);let mut mesh=mesh;mesh.add_rect_with_uv(rect,egui::Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1.,1.)),Color32::WHITE);painter.add(mesh);}}
            }
        }}
        if let(Some(response),Some(portrait))=(&popout,portrait_marker){crate::presence::paint_badge(&ctx.layer_painter(response.response.layer_id).with_clip_rect(response.response.rect),portrait,self.presences.get(&user.id,self.profile_guild.as_deref()));}
        if close||!open{self.profile=None;self.profile_anchor=None;}
        if full{self.open_full_profile(&user);}
        popout.map(|p|p.response.rect)
    }
    pub(in crate::ui) fn guild_tag_chip(&mut self,ui:&mut egui::Ui,user:&User){
        if let Some((tag,url))=crate::identity::guild_tag(user){egui::Frame::NONE.fill(CARD).corner_radius(4).inner_margin(egui::Margin::symmetric(4,2)).show(ui,|ui|{ui.horizontal(|ui|{ui.spacing_mut().item_spacing.x=3.;if url.is_some(){let(rect,_)=ui.allocate_exact_size(Vec2::splat(13.),egui::Sense::hover());crate::identity::paint_art(ui,&mut self.images,rect,url,0);}ui.label(RichText::new(tag).size(10.).strong().color(TEXT));});});}
    }
    /// A voice channel (or someone listed under it) as a drop target for a dragged person:
    /// highlights while hovered and moves them on release.
    pub(in crate::ui) fn voice_drop_target(&mut self,ui:&egui::Ui,response:&egui::Response,channel:&Channel){
        if let Some(drag)=response.dnd_hover_payload::<VoiceDrag>().filter(|d|d.from!=channel.id){
            ui.painter().rect_stroke(response.rect.expand(1.),6,Stroke::new(1.5_f32,self.accent()),egui::StrokeKind::Inside);
            let _=drag;
        }
        if let Some(drag)=response.dnd_release_payload::<VoiceDrag>(){
            if drag.from!=channel.id&&channel.guild_id.as_deref()==Some(drag.guild.as_str()){
                if self.preview{self.error=Some("Offline preview: voice moderation is not sent to Discord.".into());}
                else{self.mutate("voice-moderation",reqwest::Method::PATCH,format!("/guilds/{}/members/{}",drag.guild,drag.user),Some(serde_json::json!({"channel_id":channel.id})));}
            }
        }
    }
    /// The dragged person's name follows the pointer while moving them between voice channels.
    pub(in crate::ui) fn voice_drag_overlay(&self,ctx:&egui::Context){
        let Some(drag)=egui::DragAndDrop::payload::<VoiceDrag>(ctx) else{return};
        let Some(pointer)=ctx.pointer_latest_pos() else{return};
        egui::Area::new(egui::Id::new("voice-drag")).order(egui::Order::Tooltip).interactable(false).fixed_pos(pointer+Vec2::new(14.,8.)).show(ctx,|ui|{
            egui::Frame::NONE.fill(CARD).stroke(Stroke::new(1.0_f32,BORDER)).corner_radius(6).inner_margin(egui::Margin::symmetric(8,4)).show(ui,|ui|{ui.label(RichText::new(format!("Move {}",drag.name)).size(12.).color(TEXT));});
        });
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    /// Gives the call screen the same people the sidebar lists under the call's voice channel.
    pub(in crate::ui) fn sync_call_roster(&mut self){
        let Some(channel)=self.calls.channel().cloned() else{return};
        let states=self.voice.in_channel(&channel.id);
        let users=states.iter().map(|state|(self.voice_identity(state).0,state.streaming)).collect();
        self.calls.sync_roster(users);
        let me=self.user.as_ref().map(|u|u.id.clone()).unwrap_or_default();
        let previews=states.iter().filter(|s|s.streaming&&s.user_id!=me).filter_map(|s|self.stream_preview(&channel,&s.user_id).map(|url|(s.user_id.clone(),url))).collect();
        self.calls.previews=previews;
    }
    /// The current preview picture of someone's stream, asked for again at most once a minute.
    pub(in crate::ui) fn stream_preview(&mut self,channel:&Channel,user:&str)->Option<String>{
        if self.preview{return None;}
        let key=crate::calls::stream_key(channel,user);
        let feature=format!("stream-preview:{key}");
        if self.stream_preview_fetched.get(&key).is_none_or(|at|at.elapsed()>=Duration::from_secs(60)){
            if self.stream_preview_fetched.len()>256{self.stream_preview_fetched.clear();}
            self.stream_preview_fetched.insert(key.clone(),Instant::now());
            self.request_feature(&feature,format!("/streams/{key}/preview"));
        }
        self.stream_preview_status(channel,user).ok()
    }
    /// The preview picture link, or why there isn't one (shown in the hover popup).
    pub(in crate::ui) fn stream_preview_status(&self,channel:&Channel,user:&str)->Result<String,String>{
        let feature=format!("stream-preview:{}",crate::calls::stream_key(channel,user));
        if let Some(error)=self.feature_errors.get(&feature){
            return Err(if error.contains("404")||error.to_lowercase().contains("unknown"){"No preview yet".into()}else{format!("Preview unavailable: {}",error.chars().take(80).collect::<String>())});
        }
        let Some(data)=self.features.get(&feature) else{return Err("Loading preview…".into())};
        let Some(url)=data["url"].as_str() else{return Err("No preview yet".into())};
        if !assets::public_url(url)&&!assets::stream_preview_url(url){
            let host=reqwest::Url::parse(url).ok().and_then(|u|u.host_str().map(str::to_owned)).unwrap_or_default();
            return Err(format!("Preview hosted on {host} is not loaded"));
        }
        Ok(url.to_owned())
    }
    /// Watch Stream from the sidebar or a call tile: joins the voice channel first if needed.
    pub(in crate::ui) fn watch_stream(&mut self,channel:&Channel,user:&str){
        if self.calls.channel().is_some_and(|c|c.id==channel.id){
            if let Err(error)=self.calls.watch(user){self.error=Some(error.into());}
            return;
        }
        if self.calls.active(){self.calls.leave();}
        self.start_call(channel,false);
        if self.calls.channel().is_some_and(|c|c.id==channel.id){self.calls.watch_when_ready(user);}
    }
    /// Everyone in a voice channel, from any Discord client, listed under the channel like Discord.
    pub(in crate::ui) fn voice_member_row(&mut self,ui:&mut egui::Ui,channel:&Channel){
        if channel.kind!=2{return;}
        let in_call=self.calls.channel().is_some_and(|c|c.id==channel.id);
        let me=self.user.as_ref().map(|u|u.id.clone()).unwrap_or_default();
        let mut states=self.voice.in_channel(&channel.id);
        // While Eclipse is still connecting, Discord may not have echoed our own voice state yet.
        if in_call&&!states.iter().any(|s|s.user_id==me){states.insert(0,crate::voice_roster::VoiceState{user_id:me.clone(),guild_id:channel.guild_id.clone(),channel_id:channel.id.clone(),..Default::default()});}
        if states.is_empty(){return;}
        let mut missing=vec![];
        let mut watch=None;let mut hovered_live=None;let mut moderation=None;
        // Voice moderation needs the matching server permission (Mute, Deafen, Move Members).
        let here=channel.guild_id.as_deref().is_some_and(|g|g==self.server.id);
        let (can_mute,can_deafen,can_move)=(here&&self.server.can(&me,22),here&&self.server.can(&me,23),here&&self.server.can(&me,24));
        let destinations:Vec<(String,String)>=if can_move{self.channels.iter().filter(|c|matches!(c.kind,2|13)&&c.id!=channel.id&&c.guild_id==channel.guild_id).map(|c|(c.id.clone(),c.label())).collect()}else{vec![]};
        for state in states.iter().take(99){
            let (user,name)=self.voice_identity(state);
            if user.username.is_empty(){missing.push(state.user_id.clone());}
            let own=in_call&&state.user_id==me;
            // The green ring shows whoever is talking, like Discord; only known while in the call.
            let speaking=in_call&&self.calls.is_speaking(&state.user_id);
            // Hovering someone who is live offers Watch Stream, like Discord.
            let row=egui::Rect::from_min_size(ui.cursor().min,Vec2::new(ui.available_width(),22.));
            let offer=state.streaming&&!own&&ui.rect_contains_pointer(row)&&!(in_call&&self.calls.watching()==Some(state.user_id.as_str()));
            if offer{hovered_live=Some((state.user_id.clone(),row));}
            let (muted,deafened)=if own{(self.calls.muted(),self.calls.deafened())}else{(state.muted,state.deafened)};
            // Right-clicking anywhere on the row opens the person's menu plus voice moderation.
            let row_response=ui.interact(row,egui::Id::new(("voice-row",&channel.id,&state.user_id)),if can_move{egui::Sense::click_and_drag()}else{egui::Sense::click()});
            // With Move Members, drag someone onto another voice channel to move them, like Discord.
            if can_move&&row_response.drag_started(){
                if let Some(guild)=channel.guild_id.clone(){egui::DragAndDrop::set_payload(ui.ctx(),VoiceDrag{user:state.user_id.clone(),name:name.clone(),from:channel.id.clone(),guild});}
            }
            // Dropping onto someone already in this channel moves the dragged person here too.
            self.voice_drop_target(ui,&row_response,channel);
            ui.horizontal(|ui|{
                ui.spacing_mut().item_spacing.x=6.;ui.add_space(26.);
                let before=ui.cursor().min;
                let avatar=self.paint_avatar(ui,&user,None,22.,false);
                if avatar.clicked(){self.toggle_profile_at(&user,avatar.rect);}
                avatar.context_menu(|ui|{self.user_menu(ui,&user);if let Some(choice)=voice_moderation_menu(ui,state,can_mute,can_deafen,can_move,&destinations){moderation=Some(choice);}});
                if speaking{ui.painter().circle_stroke(before+Vec2::splat(11.),12.,Stroke::new(2.0_f32,Color32::from_rgb(67,181,129)));}
                // Reserve room for the state icons so long names truncate instead of pushing them off.
                let icons=if own{22.}else{if deafened||muted{20.}else{0.}}+if state.video{20.}else{0.}+if offer{86.}else if state.streaming{34.}else{0.};
                ui.allocate_ui_with_layout(Vec2::new((ui.available_width()-icons).max(20.),22.),egui::Layout::left_to_right(egui::Align::Center),|ui|{
                    ui.add(egui::Label::new(RichText::new(&name).size(12.).color(if speaking{TEXT}else{MUTED})).truncate());
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                    ui.spacing_mut().item_spacing.x=2.;
                    use crate::widgets::{bar_control,Control};
                    if own{if bar_control(ui,Control::Mic,muted,20.,if muted{"Unmute"}else{"Mute"}).clicked(){self.calls.toggle_mute();}}
                    else{
                        if deafened{bar_control(ui,Control::Headphones,true,18.,"Deafened");}
                        if muted&&!deafened{bar_control(ui,Control::Mic,true,18.,"Muted");}
                    }
                    if state.video{bar_control(ui,Control::Camera,false,18.,"Camera on");}
                    if offer{
                        let (rect,_)=ui.allocate_exact_size(Vec2::new(84.,18.),egui::Sense::hover());
                        if crate::calls::watch_pill(ui,rect,egui::Id::new(("sidebar-watch",&state.user_id)),"Watch Stream"){watch=Some(state.user_id.clone());}
                    }else if state.streaming{
                        let galley=ui.painter().layout_no_wrap("LIVE".into(),egui::FontId::proportional(9.),Color32::WHITE);
                        let (rect,response)=ui.allocate_exact_size(galley.size()+Vec2::new(8.,4.),egui::Sense::hover());
                        ui.painter().rect_filled(rect,4,Color32::from_rgb(218,55,60));ui.painter().galley(rect.center()-galley.size()/2.,galley,Color32::WHITE);
                        response.on_hover_text("Streaming");
                    }
                });
            });
            row_response.context_menu(|ui|{self.user_menu(ui,&user);if let Some(choice)=voice_moderation_menu(ui,state,can_mute,can_deafen,can_move,&destinations){moderation=Some(choice);}});
        }
        if let Some(user)=watch{self.watch_stream(channel,&user);}
        if let (Some((user,change)),Some(guild))=(moderation,channel.guild_id.clone()){
            if self.preview{self.error=Some("Offline preview: voice moderation is not sent to Discord.".into());}
            else{self.mutate("voice-moderation",reqwest::Method::PATCH,format!("/guilds/{guild}/members/{user}"),Some(change));}
        }
        if let Some((user,row))=hovered_live{
            let _=self.stream_preview(channel,&user);
            let status=self.stream_preview_status(channel,&user);
            {
                let size=Vec2::new(256.,144.);
                egui::Area::new(egui::Id::new("stream-preview-popup")).order(egui::Order::Tooltip).interactable(false).fixed_pos(row.right_top()+Vec2::new(14.,-60.)).show(ui.ctx(),|ui|{
                    egui::Frame::NONE.fill(CARD).stroke(Stroke::new(1.0_f32,BORDER)).corner_radius(8).inner_margin(6).show(ui,|ui|{
                        let (rect,_)=ui.allocate_exact_size(size,egui::Sense::hover());
                        ui.painter().rect_filled(rect,6,Color32::BLACK);
                        // Always say what is happening, so a missing preview can be explained.
                        let message=match &status{
                            Ok(url)=>match self.images.texture(url,rect,ui.ctx()){
                                Some(texture)=>{
                                    let image=self.images.dimensions(url,rect.size(),ui.ctx()).unwrap_or(rect.size());
                                    egui::Image::new((texture,rect.size())).uv(crate::identity::cover_uv(image,rect.size())).corner_radius(6).paint_at(ui,rect);
                                    None
                                }
                                None if self.images.failed(url)=>Some("Preview picture failed to load".to_owned()),
                                None=>Some("Loading preview…".to_owned()),
                            },
                            Err(message)=>Some(message.clone()),
                        };
                        if let Some(message)=message{
                            let text=ui.painter().layout(message,egui::FontId::proportional(12.),MUTED,rect.width()-20.);
                            ui.painter().galley(rect.center()-text.size()/2.,text,MUTED);
                        }
                    });
                });
            }
        }
        if let Some(guild)=channel.guild_id.clone().filter(|_|!self.preview){
            let ids:Vec<String>=missing.into_iter().filter(|id|self.voice_lookups.insert(id.clone())).take(100).collect();
            if !ids.is_empty()&&self.voice_lookups.len()<5000{self.send_gateway(serde_json::json!({"op":8,"d":{"guild_id":guild,"user_ids":ids,"presences":false}}));}
        }
        if in_call{
            if self.voice_revealed.as_deref()!=Some(&channel.id){ui.scroll_to_cursor(Some(egui::Align::Center));self.voice_revealed=Some(channel.id.clone());}
            if !self.preview {ui.ctx().request_repaint_after(Duration::from_millis(100));}
        }
    }
    /// The best-known user and display name for a voice participant.
    fn voice_identity(&self,state:&crate::voice_roster::VoiceState)->(User,String){
        let member=self.server.members.get(&state.user_id).filter(|_|state.guild_id.as_deref()==Some(self.server.id.as_str()));
        let user=state.user.clone()
            .or_else(||member.map(|m|m.user.clone()))
            .or_else(||self.user.clone().filter(|u|u.id==state.user_id))
            .or_else(||self.friends.iter().find(|r|r.id==state.user_id&&!r.user.username.is_empty()).map(|r|r.user.clone()))
            .or_else(||self.dms.iter().flat_map(|c|c.recipients.iter()).find(|u|u.id==state.user_id).cloned())
            .or_else(||self.messages.iter().map(|m|&m.author).find(|u|u.id==state.user_id).cloned())
            .unwrap_or_else(||User{id:state.user_id.clone(),..Default::default()});
        let name=state.nick.clone().or_else(||member.and_then(|m|m.nick.clone())).unwrap_or_else(||if user.username.is_empty(){"Loading…".into()}else{user.name().to_owned()});
        (user,name)
    }
    pub(in crate::ui) fn voice_connection_card(&mut self,ui:&mut egui::Ui){
        let Some(channel)=self.calls.channel().cloned()else{return;};
        egui::Frame::NONE.fill(SIDE).corner_radius(10).inner_margin(10).show(ui,|ui|{
            ui.set_min_width(ui.available_width());ui.horizontal(|ui|{
                ui.vertical(|ui|{ui.label(RichText::new(if self.preview{"Voice preview"}else{self.calls.connection_label()}).size(12.).strong().color(Color32::from_rgb(94,200,148)));ui.add(egui::Label::new(RichText::new(channel.label()).size(11.).color(MUTED)).truncate());});
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if crate::widgets::control(ui,crate::widgets::Control::Hangup,false,28.,"Disconnect from voice").clicked(){self.calls.leave();}if ui.small_button("View").on_hover_text("Open call view").clicked(){self.calls.chat=false;}});
            });
        });ui.add_space(8.);
    }
}
pub(in crate::ui) fn profile_key_for(id:&str,guild:Option<&str>)->String{format!("profile:{id}:{}",guild.unwrap_or("@me"))}
pub(in crate::ui) fn profile_route(id:&str,guild:Option<&str>)->String{format!("/users/{id}/profile?with_mutual_guilds=true&with_mutual_friends_count=true{}",guild.map(|g|format!("&guild_id={g}")).unwrap_or_default())}
pub(in crate::ui) fn profile_metadata(data:&Value)->Value {
    let mut metadata=data["user_profile"].clone();
    if let Some(server)=data["guild_member_profile"].as_object(){if !metadata.is_object(){metadata=json!({});}for(key,value)in server{if !value.is_null(){metadata[key]=value.clone();}}}
    metadata
}

/// Discord's voice moderation items for someone under a voice channel. Returns the member
/// change to send (PATCH /guilds/{guild}/members/{user}) when one is chosen.
pub(in crate::ui) fn voice_moderation_menu(ui:&mut egui::Ui,state:&crate::voice_roster::VoiceState,can_mute:bool,can_deafen:bool,can_move:bool,destinations:&[(String,String)])->Option<(String,serde_json::Value)>{
    if !(can_mute||can_deafen||can_move){return None;}
    let mut change=None;
    ui.separator();
    if can_mute&&ui.checkbox(&mut state.server_muted.clone(),"Server Mute").clicked(){change=Some(serde_json::json!({"mute":!state.server_muted}));ui.close();}
    if can_deafen&&ui.checkbox(&mut state.server_deafened.clone(),"Server Deafen").clicked(){change=Some(serde_json::json!({"deaf":!state.server_deafened}));ui.close();}
    if can_move{
        ui.menu_button("Move To",|ui|{
            if destinations.is_empty(){ui.weak("No other voice channels");}
            egui::ScrollArea::vertical().max_height(300.).show(ui,|ui|{
                for (id,label) in destinations{if ui.button(label).clicked(){change=Some(serde_json::json!({"channel_id":id}));ui.close();}}
            });
        });
        if ui.button(RichText::new("Disconnect").color(Color32::from_rgb(242,63,67))).clicked(){change=Some(serde_json::json!({"channel_id":null}));ui.close();}
    }
    change.map(|change|(state.user_id.clone(),change))
}

/// Someone being dragged to another voice channel.
pub(in crate::ui) struct VoiceDrag { user: String, name: String, from: String, guild: String }
