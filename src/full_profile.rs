use super::*;
use super::profile_popout::{profile_key_for, profile_metadata, profile_route};
impl Eclipse {
    /// Opens the large, centred profile (right-click → Profile, or the avatar in a profile popout).
    pub(in crate::ui) fn open_full_profile(&mut self,user:&User){
        self.profile=None;self.profile_anchor=None;
        self.full_profile=Some(user.clone());self.full_profile_guild=self.guild.clone();self.full_profile_tab=0;
        let guild=self.full_profile_guild.clone();
        self.request_feature(&profile_key_for(&user.id,guild.as_deref()),profile_route(&user.id,guild.as_deref()));
        if self.user.as_ref().is_none_or(|me|me.id!=user.id){self.request_feature(&format!("mutual-friends:{}",user.id),format!("/users/{}/relationships",user.id));}
    }
    pub(in crate::ui) fn full_profile_window(&mut self,ctx:&egui::Context){
        let Some(user)=self.full_profile.clone() else{return;};
        let guild=self.full_profile_guild.clone();
        let data=self.features.get(&profile_key_for(&user.id,guild.as_deref())).cloned().unwrap_or(Value::Null);
        let user=crate::identity::merged(&user,&data);let p=profile_metadata(&data);
        let (primary,top,bottom)=self.profile_palette(&user,&p);
        let banner_url=crate::identity::banner(&user,&data,guild.as_deref());
        let screen=ctx.screen_rect();
        // The banner (or the profile colours) fills the screen behind the dimmed modal backdrop.
        let backdrop=ctx.layer_painter(egui::LayerId::new(egui::Order::Middle,egui::Id::new("full-profile-backdrop")));
        crate::identity::vertical_gradient(&backdrop,screen,top,bottom,0.);
        if let Some(url)=banner_url.clone(){if let Some(texture)=self.images.texture_hover(&url,screen,ctx){let uv=crate::identity::cover_uv(self.images.dimensions(&url,screen.size(),ctx).unwrap_or(screen.size()),screen.size());backdrop.image(texture,screen,uv,Color32::from_gray(120));}}
        let size=Vec2::new((screen.width()-80.).clamp(560.,960.),(screen.height()-80.).clamp(420.,700.));
        let left=(size.x*0.44).min(400.);
        let mut close=false;let mut message=false;let mut call=false;let mut open_other=None;
        let response=egui::Modal::new(egui::Id::new("full-profile")).backdrop_color(Color32::from_black_alpha(150))
            .frame(egui::Frame::NONE.corner_radius(20))
            .show(ctx,|ui|{
                let outer=egui::Rect::from_min_size(ui.cursor().min,size);
                ui.set_width(size.x);ui.set_height(size.y);
                crate::identity::vertical_gradient(ui.painter(),outer,top.lerp_to_gamma(Color32::BLACK,0.55),bottom.lerp_to_gamma(Color32::BLACK,0.55),20.);
                ui.painter().rect_stroke(outer,20,Stroke::new(1.0_f32,primary.gamma_multiply(0.45)),egui::StrokeKind::Inside);
                let card=egui::Rect::from_min_size(outer.min+Vec2::splat(12.),Vec2::new(left-24.,size.y-24.));
                crate::identity::vertical_gradient(ui.painter(),card,top,bottom,16.);
                ui.scope_builder(egui::UiBuilder::new().max_rect(card),|ui|{
                    egui::ScrollArea::vertical().id_salt("full-profile-card").auto_shrink([false,false]).show(ui,|ui|{
                        ui.spacing_mut().item_spacing.y=6.;
                        let (banner,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),150.),egui::Sense::hover());
                        ui.painter().rect_filled(banner,egui::CornerRadius{nw:16,ne:16,sw:0,se:0},primary);
                        crate::identity::paint_art(ui,&mut self.images,banner,banner_url.clone(),16);
                        let fade_to=top.lerp_to_gamma(bottom,((banner.bottom()-card.top())/card.height()).clamp(0.,1.));
                        crate::identity::vertical_gradient(ui.painter(),banner.with_min_y(banner.top()+40.).expand2(Vec2::new(0.,1.)),Color32::TRANSPARENT,fade_to,0.);
                        let portrait=egui::Rect::from_min_size(egui::pos2(banner.left()+20.,banner.bottom()-58.),Vec2::splat(104.));
                        ui.painter().circle_filled(portrait.center(),58.,fade_to);ui.painter().circle_filled(portrait.center(),52.,CARD);
                        let url=if self.preview{Some(format!("demo://user/{}",user.id))}else{assets::avatar_url(&user,guild.as_deref(),data["guild_member"]["avatar"].as_str())};
                        self.paint_image(ui,portrait,url,52);
                        let mut decorated=user.clone();if let Some(value)=data["guild_member"]["avatar_decoration_data"].as_object(){decorated.avatar_decoration_data=Some(Value::Object(value.clone()));}
                        crate::identity::paint_art_playing(ui,&mut self.images,portrait.expand(10.),crate::identity::decoration(&decorated),0);
                        crate::presence::badge(ui,portrait,self.presences.get(&user.id,guild.as_deref()));
                        ui.add_space(52.);
                        egui::Frame::NONE.inner_margin(egui::Margin::symmetric(18,4)).show(ui,|ui|{
                            ui.set_width(ui.available_width());
                            let nick=data["guild_member"]["nick"].as_str().unwrap_or_else(||user.name()).to_owned();
                            ui.horizontal(|ui|{crate::identity::name(ui,&user,&nick,26.,TEXT);self.guild_tag_chip(ui,&user);});
                            let pronouns=p["pronouns"].as_str().or(user.pronouns.as_deref()).filter(|s|!s.is_empty());
                            ui.label(RichText::new(match pronouns{Some(p)=>format!("{}  •  {p}",user.username),None=>user.username.clone()}).size(13.).color(Color32::from_gray(215)));
                            let badges:Vec<Value>=data["badges"].as_array().into_iter().flatten().chain(data["guild_badges"].as_array().into_iter().flatten()).take(24).cloned().collect();
                            if !badges.is_empty(){ui.horizontal_wrapped(|ui|{ui.spacing_mut().item_spacing=Vec2::new(5.,4.);for b in &badges{let(rect,response)=ui.allocate_exact_size(Vec2::splat(22.),egui::Sense::hover());crate::identity::paint_art(ui,&mut self.images,rect,b["icon"].as_str().and_then(crate::identity::badge),0);response.on_hover_text(b["description"].as_str().unwrap_or("Profile badge"));}});}
                            ui.add_space(6.);
                            let own=self.user.as_ref().is_some_and(|u|u.id==user.id);
                            ui.horizontal(|ui|{
                                if own{if ui.add_sized([150.,32.],primary_button("Edit Profile")).clicked(){self.open_settings();self.settings_page="Account & Profile".into();close=true;}}
                                else{if ui.add_sized([130.,32.],primary_button("Message")).clicked(){message=true;}if ui.add_sized([60.,32.],egui::Button::new("Call")).clicked(){call=true;}}
                                ui.menu_button(RichText::new("•••").size(14.),|ui|self.user_menu(ui,&user));
                            });
                            ui.add_space(8.);
                            if let Some(bio)=p["bio"].as_str().or(user.bio.as_deref()).filter(|s|!s.is_empty()){section(ui,if data["guild_member_profile"]["bio"].as_str().is_some_and(|b|!b.is_empty()){"Server Bio"}else{"About Me"});crate::message_media::body(ui,&mut self.images,bio);ui.add_space(6.);}
                            section(ui,"Member Since");
                            ui.horizontal(|ui|{
                                let discord=snowflake_date(&user.id);
                                if let Some(date)=&discord{ui.label(RichText::new(date).size(13.)).on_hover_text("Joined Discord");}
                                if let Some(date)=data["guild_member"]["joined_at"].as_str().and_then(iso_date){let server=guild.as_ref().and_then(|id|self.guilds.iter().find(|g|&g.id==id)).map(|g|g.name.clone()).unwrap_or("this server".into());if discord.is_some(){ui.label(RichText::new("•").color(MUTED));}ui.label(RichText::new(date).size(13.)).on_hover_text(format!("Joined {server}"));}
                            });
                            ui.add_space(6.);
                            if let Some(member)=self.server.members.get(&user.id).cloned().filter(|_|guild.as_deref()==Some(&self.server.id)){
                                let roles:Vec<_>=self.server.roles.iter().filter(|r|member.roles.contains(&r.id)).cloned().collect();
                                if !roles.is_empty(){section(ui,"Roles");ui.horizontal_wrapped(|ui|{for role in roles{egui::Frame::NONE.fill(Color32::from_black_alpha(70)).corner_radius(6).inner_margin(egui::Margin::symmetric(8,4)).show(ui,|ui|{ui.horizontal(|ui|{let(rect,_)=ui.allocate_exact_size(Vec2::splat(10.),egui::Sense::hover());ui.painter().circle_filled(rect.center(),5.,if role.rgb()==0{MUTED}else{crate::identity::color(role.rgb())});ui.label(RichText::new(&role.name).size(12.));});});}});ui.add_space(6.);}
                            }
                            if let Some(accounts)=data["connected_accounts"].as_array().filter(|a|!a.is_empty()){
                                section(ui,"Connections");
                                for account in accounts.iter().take(20){egui::Frame::NONE.fill(Color32::from_black_alpha(60)).corner_radius(8).inner_margin(egui::Margin::symmetric(10,7)).show(ui,|ui|{ui.set_min_width(ui.available_width());ui.horizontal(|ui|{
                                    ui.label(RichText::new(service_name(account["type"].as_str().unwrap_or(""))).size(11.).strong().color(Color32::from_gray(200)));
                                    ui.label(RichText::new(account["name"].as_str().unwrap_or("")).size(13.));
                                    if account["verified"].as_bool()==Some(true){ui.label(RichText::new("✔").size(11.).color(Color32::from_rgb(88,200,140))).on_hover_text("Verified");}
                                });});}
                            }
                            self.feature_status(ui,&profile_key_for(&user.id,guild.as_deref()));
                            if self.preview{ui.weak("Sample profile · offline preview");}
                            ui.add_space(10.);
                        });
                    });
                });
                // Right side: mutual servers and friends.
                let side=egui::Rect::from_min_max(egui::pos2(card.right()+16.,outer.top()+16.),outer.right_bottom()-Vec2::new(16.,16.));
                ui.scope_builder(egui::UiBuilder::new().max_rect(side),|ui|{
                    let friends:Vec<User>=self.features.get(&format!("mutual-friends:{}",user.id)).and_then(|v|serde_json::from_value(v.clone()).ok()).unwrap_or_default();
                    let servers:Vec<(Guild,Option<String>)>=data["mutual_guilds"].as_array().into_iter().flatten().filter_map(|m|{let id=m["id"].as_str()?;self.guilds.iter().find(|g|g.id==id).cloned().map(|g|(g,m["nick"].as_str().map(str::to_owned)))}).collect();
                    ui.horizontal(|ui|{
                        for (tab,label) in [(0u8,format!("Mutual Servers  {}",servers.len())),(1,format!("Mutual Friends  {}",friends.len()))]{
                            let selected=self.full_profile_tab==tab;
                            let response=ui.add(egui::Label::new(RichText::new(label).size(14.).strong().color(if selected{TEXT}else{MUTED})).sense(egui::Sense::click()));
                            if selected{ui.painter().line_segment([response.rect.left_bottom()+Vec2::new(0.,6.),response.rect.right_bottom()+Vec2::new(0.,6.)],Stroke::new(2.0_f32,primary.lerp_to_gamma(Color32::WHITE,0.4)));}
                            if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked(){self.full_profile_tab=tab;}
                            ui.add_space(14.);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if ui.add_sized([30.,30.],egui::Button::new(RichText::new("×").size(20.)).fill(Color32::from_black_alpha(60))).on_hover_text("Close · Escape").clicked(){close=true;}});
                    });
                    ui.add_space(8.);ui.separator();ui.add_space(4.);
                    egui::ScrollArea::vertical().id_salt("full-profile-side").auto_shrink([false,false]).show(ui,|ui|{
                        let own=self.user.as_ref().is_some_and(|u|u.id==user.id);
                        if own{ui.add_space(20.);ui.vertical_centered(|ui|ui.label(RichText::new("Mutual servers and friends appear on other people's profiles.").color(MUTED)));return;}
                        if self.full_profile_tab==0{
                            if servers.is_empty(){ui.add_space(20.);ui.vertical_centered(|ui|ui.label(RichText::new("No servers in common").color(MUTED)));}
                            for (server,nick) in servers{row(ui,|ui|{let(rect,_)=ui.allocate_exact_size(Vec2::splat(36.),egui::Sense::hover());ui.painter().rect_filled(rect,10,CARD);ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,initials_of(&server.name),egui::FontId::proportional(11.),TEXT);let url=self.guild_image_url(&server);self.paint_image(ui,rect,url,10);ui.vertical(|ui|{ui.label(RichText::new(&server.name).size(14.).strong());if let Some(nick)=nick{ui.label(RichText::new(nick).size(11.).color(MUTED));}});});}
                        }else{
                            self.feature_status(ui,&format!("mutual-friends:{}",user.id));
                            if friends.is_empty()&&!self.feature_pending.contains(&format!("mutual-friends:{}",user.id)){ui.add_space(20.);ui.vertical_centered(|ui|ui.label(RichText::new("No friends in common").color(MUTED)));}
                            for friend in friends{let clicked=row(ui,|ui|{self.user_avatar(ui,&friend,None,36.);ui.label(RichText::new(friend.name()).size(14.).strong());});if clicked{open_other=Some(friend);}}
                        }
                    });
                });
            });
        if message{self.message_person(&user);close=true;}
        if call{if let Some(dm)=self.dms.iter().find(|c|c.kind==1&&c.recipients.iter().any(|u|u.id==user.id)).cloned(){if self.preview{self.calls.preview(dm,self.user.clone().unwrap_or_default(),vec![user.clone()]);}else{self.start_call(&dm,false);}close=true;}else{self.error=Some("Open a direct message first to call this person.".into());}}
        if close||response.should_close(){self.full_profile=None;}
        if let Some(other)=open_other{self.open_full_profile(&other);}
    }
}
fn section(ui:&mut egui::Ui,title:&str){ui.label(RichText::new(title).size(12.).strong().color(Color32::from_gray(225)));}
fn primary_button(text:&str)->egui::Button<'_>{egui::Button::new(RichText::new(text).strong().color(Color32::from_gray(20))).fill(Color32::from_gray(232)).corner_radius(8)}
/// A hoverable list row; returns whether it was clicked.
fn row(ui:&mut egui::Ui,content:impl FnOnce(&mut egui::Ui))->bool{
    let bg=ui.painter().add(egui::Shape::Noop);
    let inner=egui::Frame::NONE.inner_margin(egui::Margin::symmetric(8,6)).show(ui,|ui|{ui.set_min_width(ui.available_width());ui.horizontal(content);}).response;
    let response=ui.interact(inner.rect,ui.id().with(("profile-row",inner.rect.min.y as i32)),egui::Sense::click());
    if response.hovered(){ui.painter().set(bg,egui::Shape::rect_filled(inner.rect,8,Color32::from_white_alpha(14)));}
    response.clicked()
}
fn service_name(kind:&str)->String{match kind{"xbox"=>"Xbox".into(),"playstation"=>"PlayStation".into(),"youtube"=>"YouTube".into(),"github"=>"GitHub".into(),"tiktok"=>"TikTok".into(),"leagueoflegends"=>"League of Legends".into(),"battlenet"=>"Battle.net".into(),"riotgames"=>"Riot Games".into(),other=>{let mut c=other.chars();c.next().map(|f|f.to_uppercase().collect::<String>()+c.as_str()).unwrap_or_default()}}}
fn date_label(y:i64,m:u32,d:u32)->String{const MONTHS:[&str;12]=["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"];format!("{} {d}, {y}",MONTHS[(m.clamp(1,12)-1) as usize])}
/// Account creation date from a Discord snowflake (UTC).
fn snowflake_date(id:&str)->Option<String>{
    let ms=(id.parse::<u64>().ok()?>>22)+1_420_070_400_000;let z=(ms/86_400_000) as i64+719_468;
    let era=z.div_euclid(146_097);let doe=z-era*146_097;let yoe=(doe-doe/1460+doe/36_524-doe/146_096)/365;
    let doy=doe-(365*yoe+yoe/4-yoe/100);let mp=(5*doy+2)/153;let d=(doy-(153*mp+2)/5+1) as u32;let m=(if mp<10{mp+3}else{mp-9}) as u32;
    Some(date_label(yoe+era*400+i64::from(m<=2),m,d))
}
fn iso_date(s:&str)->Option<String>{let y=s.get(0..4)?.parse().ok()?;let m=s.get(5..7)?.parse().ok()?;let d=s.get(8..10)?.parse().ok()?;(1..=12).contains(&m).then(||date_label(y,m,d))}
#[cfg(test)] mod tests {
    #[test]fn dates_from_snowflakes_and_iso_timestamps(){
        assert_eq!(super::snowflake_date("175928847299117063").as_deref(),Some("Apr 30, 2016"));
        assert_eq!(super::iso_date("2021-10-13T01:02:03.000000+00:00").as_deref(),Some("Oct 13, 2021"));
        assert!(super::iso_date("garbage").is_none());
    }
}
