use super::*;
use crate::{community::{self,Member,Role,Relationship},preferences::{self,Preferences,USER_CATEGORIES}};
use reqwest::Method;
use serde_json::{json,Value};
#[path="home.rs"] mod home;
#[path="server_settings.rs"] mod server_settings;
#[path="interactions.rs"] mod interactions;
#[path="profile_popout.rs"] mod profile_popout;
#[path="dm_picker.rs"] mod dm_picker;
#[path="store.rs"] mod store;
#[path="user_panel.rs"] mod user_panel;
#[path="pins.rs"] mod pins;
#[path="full_profile.rs"] mod full_profile;
#[path="account_menu.rs"] mod account_menu;
#[path="login_screen.rs"] mod login_screen;
#[path="mentions.rs"] mod mentions;
pub(in crate::ui) use login_screen::LoginUi;
pub(in crate::ui) use user_panel::USER_PANEL_HEIGHT;
pub(in crate::ui) use profile_popout::profile_key_for;
#[cfg(test)] pub(in crate::ui) use profile_popout::voice_moderation_menu;

impl Eclipse {
    pub(super) fn request_feature(&mut self,key:&str,route:String){
        if self.preview||self.feature_pending.contains(key){return;}
        self.feature_pending.insert(key.into());self.feature_errors.remove(key);
        if !self.send_command(Command::Request{key:key.into(),method:Method::GET,route,body:None}){self.feature_pending.remove(key);}
    }
    pub(super) fn mutate(&mut self,key:&str,method:Method,route:String,body:Option<Value>){
        if self.preview {self.feature_errors.insert(key.into(),"Offline preview: changes are not sent or saved to Discord.".into());return;}
        if self.feature_pending.contains(key){return;}self.feature_pending.insert(key.into());self.feature_errors.remove(key);
        if !self.send_command(Command::Request{key:key.into(),method,route,body}){self.feature_pending.remove(key);}
    }
    pub(super) fn feature_result(&mut self,key:String,result:Result<Value,String>){
        self.feature_pending.remove(&key);
        let data=match result{Ok(data)=>data,Err(error)=>{self.feature_errors.insert(key,error);return;}};
        match key.as_str(){
            "friends"=>{if let Ok(mut friends)=serde_json::from_value::<Vec<Relationship>>(data.clone()){friends.truncate(6000);self.friends=friends;}},
            "settings"|"save-settings"=>{self.settings_edit=data.clone();if let Some(activity)=data["show_current_game"].as_bool(){self.game_activity=activity;}self.features.insert("settings".into(),data);return;},
            "my-profile"=>{self.account_edit=profile_fields(&data);},
            "save-profile"=>{if let Ok(user)=serde_json::from_value::<User>(data){self.user=Some(user);}self.load_my_profile();return;},
            "save-role"|"create-role"|"member-role"=>{self.load_roles();self.refresh_members();return;},
            "save-server"=>{if data["id"].as_str()!=Some(&self.server.id){return;}self.server.metadata=data.clone();self.server_edit=data.clone();if let Some(guild)=self.guilds.iter_mut().find(|g|g.id==self.server.id){if let Some(name)=data["name"].as_str(){guild.name=name.into();}if data.get("icon").is_some(){guild.icon=data["icon"].as_str().map(str::to_owned);}}return;},
            "read-all"=>{self.unread.clear();self.features.insert(key,json!({"saved":true}));return;},
            "relationship"=>{self.request_feature("friends","/users/@me/relationships".into());return;},
            "save-notifications"=>{let key=format!("server:{}:Notifications",self.server.id);self.features.insert(key,data);return;},
            "server-write"=>{self.load_server_page();return;},
            "save-channel"|"create-channel"=>{if let Some(g)=self.guild.clone(){self.send_command(Command::Channels(g));}return;},
            "quest-enroll"=>{self.request_feature("quests","/quests/@me".into());return;},
            _=>{}
        }
        if let Some(guild)=key.strip_prefix("guild:"){if guild==self.server.id{self.server.metadata=data.clone();self.server_edit=data.clone();if let Some(roles)=data.get("roles"){if let Ok(roles)=serde_json::from_value(roles.clone()){self.server.roles=roles;}}}return;}
        if let Some(guild)=key.strip_prefix("roles:"){if guild==self.server.id{if let Ok(roles)=serde_json::from_value(data){self.server.roles=roles;}}return;}
        if let Some(guild)=key.strip_prefix("me-member:"){if guild==self.server.id{if let Ok(member)=serde_json::from_value(data){self.server.insert(member);}}return;}
        if let Some(guild)=key.strip_prefix("members:"){if guild==self.server.id{if let Ok(members)=serde_json::from_value::<Vec<Member>>(data){self.server.has_more=members.len()==100;self.server.cursor=members.last().map(|m|m.user.id.clone());for member in members{self.server.insert(member);}}}return;}
        if key.starts_with("profile:") { let full=self.full_profile.as_ref().is_some_and(|u|profile_key_for(&u.id,self.full_profile_guild.as_deref())==key); let popout=self.profile.as_ref().is_some_and(|u|self.profile_key(&u.id)==key); if !popout&&!full{return;} if full{if let Some(user)=self.full_profile.clone(){self.full_profile=Some(crate::identity::merged(&user,&data));}} if let Some(user)=self.profile.clone().filter(|_|popout){self.profile=Some(crate::identity::merged(&user,&data));self.profile_patch("USER_UPDATE",&data["user"]);if self.profile_guild.as_deref()==Some(&self.server.id){if let Some(member)=self.server.members.get_mut(&user.id){if data["guild_member"].get("avatar_decoration_data").is_some(){member.avatar_decoration_data=data["guild_member"]["avatar_decoration_data"].as_object().map(|_|data["guild_member"]["avatar_decoration_data"].clone());}}}} }
        if self.features.len()>40{self.features.retain(|key,_|matches!(key.as_str(),"settings"|"my-profile"|"shop"|"quests"));}
        self.features.insert(key,data);
    }
    pub(super) fn account_event(&mut self,kind:&str,data:&Value){
        self.voice.ingest(kind,data);
        self.server.ingest(kind,data);
        if matches!(kind,"CHANNEL_CREATE"|"CHANNEL_UPDATE"|"CHANNEL_DELETE"|"GUILD_DELETE"){if let Some(id)=data["guild_id"].as_str().or_else(||if kind=="GUILD_DELETE"{data["id"].as_str()}else{None}){self.navigation.invalidate_channels(id);}}
        match kind{
            "READY"=>{
                if let Some(guilds)=data["guilds"].as_array(){for guild in guilds.iter().take(12){if let(Some(id),Some(channels))=(guild["id"].as_str(),guild["channels"].as_array()){let mut channels:Vec<Channel>=channels.iter().filter_map(|c|serde_json::from_value(c.clone()).ok()).take(500).collect();for c in &mut channels{c.guild_id=Some(id.into());}crate::navigation::sort_channels(&mut channels);self.navigation.save_channels(id,&channels);}}}
                if let Some(relationships)=data["relationships"].as_array(){self.friends=relationships.iter().filter_map(|v|serde_json::from_value(v.clone()).ok()).take(6000).collect();}
                if data["user_settings"].is_object(){self.settings_edit=data["user_settings"].clone();self.features.insert("settings".into(),data["user_settings"].clone());self.game_activity=data["user_settings"]["show_current_game"].as_bool().unwrap_or(true);}
                if let Some(guilds)=data["guilds"].as_array(){for g in guilds.iter().take(1000){if let Some(cs)=g["channels"].as_array(){for c in cs{if let(Some(id),Some(last))=(c["id"].as_str(),c["last_message_id"].as_str()){if self.read_latest.len()<10000{self.read_latest.insert(id.into(),last.into());}}}}}}
            }
            "RELATIONSHIP_ADD"|"RELATIONSHIP_UPDATE"=>{if let Ok(mut r)=serde_json::from_value::<Relationship>(data.clone()){if let Some(old)=self.friends.iter().find(|old|old.id==r.id){if r.user.username.is_empty(){r.user=old.user.clone();}}self.friends.retain(|old|old.id!=r.id);if self.friends.len()<6000{self.friends.push(r);}}},
            "RELATIONSHIP_REMOVE"=>self.friends.retain(|r|Some(r.id.as_str())!=data["id"].as_str()),
            "USER_SETTINGS_UPDATE"=>{if let Some(v)=data["show_current_game"].as_bool(){self.game_activity=v;}if let Some(status)=data["status"].as_str(){if let Some(user)=&self.user{self.presences.set(user.id.clone(),String::new(),crate::presence::Status::parse(status));}}},
            "MESSAGE_ACK"=>{if let Some(channel)=data["channel_id"].as_str(){self.unread.remove(channel);}},
            "QUESTS_USER_STATUS_UPDATE"=>{if self.home==Home::Quests{self.request_feature("quests","/quests/@me".into());}},
            _=>{}
        }
    }
    pub(super) fn load_my_profile(&mut self){if let Some(user)=&self.user{self.request_feature("my-profile",format!("/users/{}/profile?with_mutual_guilds=false&with_mutual_friends=false",user.id));}}
    pub(super) fn load_roles(&mut self){let id=self.server.id.clone();if !id.is_empty(){self.request_feature(&format!("roles:{id}"),format!("/guilds/{id}/roles"));}}
    pub(super) fn refresh_members(&mut self){let id=self.server.id.clone();if !id.is_empty(){self.request_feature(&format!("members:{id}"),format!("/guilds/{id}/members?limit=100"));}}
    pub(super) fn load_server(&mut self,id:&str){
        self.server.select(id);if let Some(g)=self.guilds.iter().find(|g|g.id==id){self.server.metadata=json!({"id":id,"name":g.name,"owner_id":g.owner_id.clone().or_else(||if g.owner{self.user.as_ref().map(|u|u.id.clone())}else{None}),"account_permissions":g.permissions});}self.server_edit=Value::Null;self.role_edit=None;self.features.remove("channel-edit");
        self.request_feature(&format!("guild:{id}"),format!("/guilds/{id}?with_counts=true"));self.load_roles();self.request_feature(&format!("me-member:{id}"),format!("/users/@me/guilds/{id}/member"));self.refresh_members();
    }
    pub(super) fn open_settings(&mut self){self.settings=true;if !self.features.contains_key("settings"){self.request_feature("settings","/users/@me/settings".into());}if !self.features.contains_key("my-profile"){self.load_my_profile();}}
    pub(super) fn open_server_settings(&mut self){self.server_settings=true;self.server_page="Overview".into();if let Some(g)=self.guild.clone(){self.load_server(&g);}}
    pub(super) fn navigate_home(&mut self,home:Home){self.remember_conversation();self.home=home;self.profile=None;self.guild=None;self.channels=self.dms.clone();self.channel=None;self.messages.clear();self.search.clear();match home{Home::Friends=>self.request_feature("friends","/users/@me/relationships".into()),Home::Shop=>self.request_feature("shop","/collectibles-shop?include_bundles=true".into()),Home::Quests=>self.request_feature("quests","/quests/@me".into()),Home::Nitro=>self.load_my_profile(),_=>{}}}
    pub(super) fn demo_community(&mut self){
        let id=self.guild.clone().unwrap_or_else(||"local-lab".into());self.server=community::Server{id:id.clone(),metadata:json!({"id":id,"name":"Eclipse Lab","owner_id":"local-me","description":"A little more room to breathe.","verification_level":1,"default_message_notifications":1,"explicit_content_filter":2,"afk_timeout":300,"preferred_locale":"en-US","approximate_member_count":8,"premium_tier":2,"premium_subscription_count":7}),..Default::default()};
        self.server.roles=vec![Role{id:id.clone(),name:"@everyone".into(),permissions:"1024".into(),..Default::default()},Role{id:"sample-admin".into(),name:"Community team".into(),color:0x41b7cd,position:2,hoist:true,permissions:"8".into(),..Default::default()},Role{id:"sample-member".into(),name:"Members".into(),color:0xd9b4ed,position:1,hoist:true,..Default::default()}];
        self.friends.clear();for (n,message) in self.messages.clone().iter().enumerate(){self.server.insert(Member{user:message.author.clone(),roles:vec![if n<2{"sample-admin"}else{"sample-member"}.into()],..Default::default()});if message.author.id!="local-me"&&!self.friends.iter().any(|r|r.id==message.author.id){self.friends.push(Relationship{id:message.author.id.clone(),user:message.author.clone(),kind:1,..Default::default()});}}
        // Sample activities so the offline preview shows the Friends "Active Now" column.
        let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d|d.as_millis() as u64).unwrap_or(0);
        let online:Vec<String>=self.friends.iter().filter(|r|matches!(self.presences.get(&r.id,None),crate::presence::Status::Online)).map(|r|r.id.clone()).take(2).collect();
        for (id,(name,ago)) in online.iter().zip([("Sample Game",4*3_600_000u64),("Another Game",30*60_000)]){self.presences.ingest(&json!({"user":{"id":id},"status":"online","activities":[{"type":0,"name":name,"timestamps":{"start":now.saturating_sub(ago)}}]}));}
        // Sample voice states: people in the Lounge from another Discord client.
        for (id,extra) in self.friends.iter().map(|r|r.id.clone()).take(2).zip([json!({"self_mute":true}),json!({"self_stream":true,"self_video":true})]){let mut state=json!({"guild_id":"local-lab","user_id":id,"channel_id":"local-voice"});if let(Some(s),Some(e))=(state.as_object_mut(),extra.as_object()){s.extend(e.clone());}self.voice.ingest("VOICE_STATE_UPDATE",&state);}
        self.server.insert(Member{user:self.user.clone().unwrap_or_default(),roles:vec!["sample-admin".into()],..Default::default()});self.server_edit=self.server.metadata.clone();self.settings_edit=json!({"status":"online","show_current_game":true,"default_guilds_restricted":false,"friend_source_flags":{"all":true,"mutual_friends":true,"mutual_guilds":true},"locale":"en-US","render_embeds":true,"gif_auto_play":true,"animate_emoji":true});self.features.insert("settings".into(),self.settings_edit.clone());self.account_edit=json!({"global_name":"You","bio":"A little more room to breathe.","pronouns":"","accent_color":4306893});
    }
    pub(super) fn note_latest(&mut self,channel:String,message:String){if self.read_latest.len()<10000||self.read_latest.contains_key(&channel){self.read_latest.insert(channel,message);}}
    pub(super) fn log_message(&mut self,message:Message,deleted:bool){if self.logs.back().is_some_and(|e|e.deleted==deleted&&e.message.id==message.id&&e.message.channel_id==message.channel_id&&e.message.content==message.content){return;}if self.logs.len()==200{self.logs.pop_front();}self.logs.push_back(Logged{message,deleted});}
    pub(super) fn feature_status(&self,ui:&mut egui::Ui,key:&str){if self.feature_pending.contains(key){ui.spinner();ui.weak("Loading from Discord…");}if let Some(error)=self.feature_errors.get(key){ui.colored_label(Color32::from_rgb(240,132,140),error);}}
    pub(super) fn read_all(&mut self){let states:Vec<_>=self.read_latest.iter().filter(|(id,last)|community::snowflake(id)&&community::snowflake(last)).take(10000).map(|(id,last)|json!({"channel_id":id,"message_id":last,"read_state_type":0})).collect();if self.preview{self.unread.clear();}else if states.is_empty(){self.error=Some("No readable channel message IDs have arrived yet. Reconnect to load read states.".into());}else{self.mutate("read-all",Method::POST,"/read-states/ack-bulk".into(),Some(json!({"read_states":states})));}}
    pub(super) fn user_menu(&mut self,ui:&mut egui::Ui,user:&User){
        if ui.button("Profile").clicked(){self.open_full_profile(user);ui.close();}
        let own=self.user.as_ref().is_some_and(|me|me.id==user.id);
        if !own{
            if ui.button("Message").clicked(){if !self.preview{self.send_command(Command::OpenDm(user.id.clone()));}ui.close();}
            if let Some(dm)=self.dms.iter().find(|c|c.kind==1&&c.recipients.iter().any(|u|u.id==user.id)).cloned(){if ui.button("Call").clicked(){self.start_call(&dm,false);ui.close();}}
            let mut volume=self.calls.volume(&user.id);if ui.add(egui::Slider::new(&mut volume,0..=if self.prefs.volume_booster{1000}else{200}).text("User volume %")).changed(){self.calls.set_volume(&user.id,volume);}
            if ui.button("Mute locally").clicked(){self.calls.set_volume(&user.id,0);ui.close();}
            ui.separator();
            let relationship=self.friends.iter().find(|r|r.id==user.id).map(|r|r.kind).unwrap_or(0);
            if ui.button(if relationship==1{"Remove friend"}else if relationship==3{"Accept friend request"}else{"Add friend"}).clicked(){if relationship==1{self.confirm=Some((format!("Remove {} from your friends?",user.name()),Command::Request{key:"relationship".into(),method:Method::DELETE,route:format!("/users/@me/relationships/{}",user.id),body:None}));}else{self.mutate("relationship",Method::PUT,format!("/users/@me/relationships/{}",user.id),Some(json!({"type":1})));}ui.close();}
            if ui.button(if relationship==2{"Unblock"}else{"Block"}).clicked(){self.confirm=Some((format!("{} {}?",if relationship==2{"Unblock"}else{"Block"},user.name()),Command::Request{key:"relationship".into(),method:if relationship==2{Method::DELETE}else{Method::PUT},route:format!("/users/@me/relationships/{}",user.id),body:if relationship==2{None}else{Some(json!({"type":2}))}}));ui.close();}
            if let(Some(actor),Some(guild))=(self.user.as_ref().map(|u|u.id.clone()),self.guild.clone()){
                if self.server.can(&actor,28)&&self.server.can_manage(&actor,&user.id){ui.menu_button("Roles",|ui|{let eligible:Vec<_>=self.server.roles.iter().filter(|r|r.id!=guild&&self.server.can_edit_role(&actor,r)).cloned().collect();for role in eligible{
                    let mut has=self.server.members.get(&user.id).is_some_and(|m|m.roles.contains(&role.id));if ui.checkbox(&mut has,&role.name).changed(){self.mutate("member-role",if has{Method::PUT}else{Method::DELETE},format!("/guilds/{guild}/members/{}/roles/{}",user.id,role.id),None);ui.close();}
                }});}
                if self.server.can_manage(&actor,&user.id){for (label,bit,body) in [("Server mute",22,json!({"mute":true})),("Server unmute",22,json!({"mute":false})),("Server deafen",23,json!({"deaf":true})),("Server undeafen",23,json!({"deaf":false}))]{if self.server.can(&actor,bit)&&ui.button(label).clicked(){self.mutate("member-role",Method::PATCH,format!("/guilds/{guild}/members/{}",user.id),Some(body));ui.close();}}
                    if self.server.can(&actor,1)&&ui.button(RichText::new("Kick from server").color(Color32::LIGHT_RED)).clicked(){self.confirm=Some((format!("Kick {} from this server?",user.name()),Command::Request{key:"member-role".into(),method:Method::DELETE,route:format!("/guilds/{guild}/members/{}",user.id),body:None}));ui.close();}
                    if self.server.can(&actor,2)&&ui.button(RichText::new("Ban from server").color(Color32::LIGHT_RED)).clicked(){self.confirm=Some((format!("Ban {} from this server? No message history will be deleted.",user.name()),Command::Request{key:"member-role".into(),method:Method::PUT,route:format!("/guilds/{guild}/bans/{}",user.id),body:Some(json!({"delete_message_seconds":0}))}));ui.close();}
                }
            }
        }
        ui.separator();if ui.button("Copy user ID").clicked(){ui.ctx().copy_text(user.id.clone());ui.close();}if ui.button("Copy profile link").clicked(){ui.ctx().copy_text(format!("https://discord.com/users/{}",user.id));ui.close();}
    }
}
fn profile_fields(data:&Value)->Value{let profile=if data["user_profile"].is_object(){&data["user_profile"]}else{data};json!({"global_name":data["user"]["global_name"].as_str().or_else(||data["global_name"].as_str()).unwrap_or(""),"bio":profile["bio"].as_str().unwrap_or(""),"pronouns":profile["pronouns"].as_str().unwrap_or(""),"accent_color":profile["accent_color"].as_u64().unwrap_or(4306893)})}
fn text(ui:&mut egui::Ui,value:&mut Value,key:&str,label:&str,multiline:bool){ui.label(RichText::new(label).strong());let mut s=value[key].as_str().unwrap_or("").to_owned();let response=if multiline{ui.add(egui::TextEdit::multiline(&mut s).desired_width(f32::INFINITY).desired_rows(3))}else{ui.add(egui::TextEdit::singleline(&mut s).desired_width(f32::INFINITY))};if response.changed(){value[key]=json!(s);}ui.add_space(7.);}
fn toggle(ui:&mut egui::Ui,value:&mut Value,key:&str,label:&str){let mut v=value[key].as_bool().unwrap_or(false);if ui.checkbox(&mut v,label).changed(){value[key]=json!(v);}}
fn number(ui:&mut egui::Ui,value:&mut Value,key:&str,label:&str,range:std::ops::RangeInclusive<i64>){let mut v=value[key].as_i64().unwrap_or(*range.start());if ui.add(egui::Slider::new(&mut v,range).text(label)).changed(){value[key]=json!(v);}}
fn choice(ui:&mut egui::Ui,value:&mut Value,key:&str,label:&str,choices:&[(i64,&str)]){let mut v=value[key].as_i64().unwrap_or(0);egui::ComboBox::from_id_salt(key).selected_text(choices.iter().find(|c|c.0==v).map(|c|c.1).unwrap_or("Unspecified")).show_ui(ui,|ui|{for (n,label) in choices{ui.selectable_value(&mut v,*n,*label);}});ui.label(label);if value[key].as_i64()!=Some(v){value[key]=json!(v);}}
fn delta(old:&Value,new:&Value)->Value{let mut result=json!({});if let Some(o)=new.as_object(){for(k,v)in o{if old.get(k)!=Some(v){result[k]=v.clone();}}}result}
fn heading(ui:&mut egui::Ui,title:&str,description:&str){ui.heading(title);ui.add_space(6.);ui.weak(description);ui.add_space(16.);}

impl Eclipse {
    pub(super) fn settings_window(&mut self,ctx:&egui::Context){
        if !self.settings{return;}
        let close=crate::settings_layout::shell(ctx,"user-settings","User Settings","Personalize your Eclipse experience",|ui,size|{
            ui.horizontal_top(|ui|{
                egui::Frame::NONE.fill(RAIL).inner_margin(16).show(ui,|ui|{ui.vertical(|ui|{ui.set_width(188.);ui.set_height(size.y-32.);
                    ui.add(egui::TextEdit::singleline(&mut self.settings_search).hint_text("Search settings").desired_width(188.));ui.add_space(8.);
                    egui::ScrollArea::vertical().id_salt("settings-nav").max_height(size.y-112.).show(ui,|ui|{
                        for(category,pages)in USER_CATEGORIES{let query=self.settings_search.to_lowercase();let visible:Vec<_>=pages.iter().filter(|p|p.to_lowercase().contains(&query)||category.to_lowercase().contains(&query)).collect();if visible.is_empty(){continue;}
                            ui.add_space(12.);ui.label(RichText::new(*category).size(10.).strong().color(MUTED));
                            for page in visible{if crate::widgets::nav_row(ui,page,self.settings_page==**page,if self.settings_page==**page{TEXT}else{MUTED}).clicked(){self.settings_page=(**page).into();self.hotkey_record=None;self.load_settings_section();}}
                        }
                    });ui.separator();if ui.button("Log out").clicked(){self.log_out();self.settings=false;}
                });});
                egui::Frame::NONE.inner_margin(egui::Margin::symmetric(22,16)).show(ui,|ui|{ui.vertical(|ui|{ui.set_width(size.x-280.);
                    egui::ScrollArea::vertical().id_salt(("settings-body",&self.settings_page)).max_height(size.y-32.).auto_shrink([false,false]).show(ui,|ui|{self.settings_body(ui);});
                });});
            });
        });
        if close{self.settings=false;self.hotkey_record=None;}
    }
    pub(super) fn load_settings_section(&mut self){
        match self.settings_page.as_str(){
            "Account & Profile"=>self.load_my_profile(),
            "Connections"=>self.request_feature("connections","/users/@me/connections".into()),
            "Devices"=>self.request_feature("devices","/auth/sessions".into()),
            "Authorized Apps"=>self.request_feature("authorized-apps","/oauth2/tokens".into()),
            "Voice & Video"=>{self.audio_devices=discord_voice::audio::devices().ok();},
            "Gift Inventory"=>self.request_feature("inventory","/users/@me/entitlements/gifts".into()),
            "Server Boost"=>self.request_feature("boosts","/users/@me/guilds/premium/subscription-slots".into()),
            "Subscriptions"=>self.request_feature("subscriptions","/users/@me/billing/subscriptions".into()),
            _=>{}
        }
    }
    pub(super) fn settings_body(&mut self,ui:&mut egui::Ui){
        if let Some(error)=self.error.clone(){ui.horizontal_wrapped(|ui|{ui.colored_label(Color32::LIGHT_RED,error);if ui.small_button("Dismiss").clicked(){self.error=None;}});}
        let page=self.settings_page.clone();heading(ui,&page,crate::settings_layout::description(&page));
        crate::settings_layout::card(ui,|ui|{
        match page.as_str(){
            "Account & Profile"=>{
                if let Some(user)=self.user.clone(){ui.horizontal(|ui|{self.user_avatar(ui,&user,None,60.);ui.vertical(|ui|{ui.strong(user.name());ui.weak(format!("@{}",user.username));ui.label(nitro_name(user.premium_type));});});ui.add_space(20.);}
                self.feature_status(ui,"my-profile");
                if self.account_edit.is_object(){text(ui,&mut self.account_edit,"global_name","Display name",false);text(ui,&mut self.account_edit,"pronouns","Pronouns",false);text(ui,&mut self.account_edit,"bio","About me",true);
                    let mut color=self.account_edit["accent_color"].as_u64().unwrap_or(4306893)as u32;let mut rgb=[(color>>16)as u8,(color>>8)as u8,color as u8];ui.horizontal(|ui|{ui.label("Profile color");if ui.color_edit_button_srgb(&mut rgb).changed(){color=((rgb[0]as u32)<<16)|((rgb[1]as u32)<<8)|(rgb[2]as u32);self.account_edit["accent_color"]=json!(color);}});
                    ui.horizontal(|ui|{if ui.button("Change avatar…").clicked(){if let Some(path)=rfd::FileDialog::new().add_filter("Image",&["png","jpg","jpeg","gif","webp"]).pick_file(){match avatar_data(&path){Ok(data)=>self.account_edit["avatar"]=json!(data),Err(error)=>{self.feature_errors.insert("save-profile".into(),error);}}}}if ui.button("Remove avatar").clicked(){self.account_edit["avatar"]=Value::Null;}});
                    if ui.add_enabled(!self.feature_pending.contains("save-profile"),primary("Save profile")).clicked(){let body=delta(&self.features.get("my-profile").map(profile_fields).unwrap_or(Value::Null),&self.account_edit);if body.as_object().is_some_and(|o|!o.is_empty()){self.mutate("save-profile",Method::PATCH,"/users/@me".into(),Some(body));}}
                }self.feature_status(ui,"save-profile");ui.add_space(18.);ui.separator();ui.strong("Password, passkeys and two-factor authentication");ui.weak("Security verification, password changes and account deletion use Discord's own account flow.");ui.hyperlink_to("Open Discord account security","https://discord.com/channels/@me");
            }
            "Voice & Video"=>{
                ui.strong("Input device");device_picker(ui,"voice-input",&mut self.prefs.input,self.audio_devices.as_ref().map(|d|d.inputs.as_slice()));ui.strong("Output device");device_picker(ui,"voice-output",&mut self.prefs.output,self.audio_devices.as_ref().map(|d|d.outputs.as_slice()));if ui.button("Refresh devices").clicked(){self.audio_devices=discord_voice::audio::devices().ok();}
                ui.add(egui::Slider::new(&mut self.prefs.input_gain,0..=200).text("Input volume %"));ui.add(egui::Slider::new(&mut self.prefs.output_gain,0..=200).text("Output volume %"));ui.separator();ui.strong("Input mode");ui.radio_value(&mut self.prefs.push_to_talk,false,"Voice Activity");ui.radio_value(&mut self.prefs.push_to_talk,true,"Push to Talk");
                ui.horizontal(|ui|{ui.label(format!("Hotkey: {}",hotkey_label(&self.prefs)));let record=ui.button(if self.hotkey_record.is_some(){"Recording… Esc cancels"}else{"Record hotkey"});if record.clicked()&&self.hotkey_record.is_none(){record.surrender_focus();self.hotkey_record=Some(crate::hotkey::Recorder::new());}});
                if let Some(recorder)=&mut self.hotkey_record{ui.colored_label(ACCENT,"Press and release a key or chord. Ctrl, Shift and Alt also work on their own.");ui.ctx().request_repaint_after(Duration::from_millis(16));
                    let down=|k|unsafe{windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(k)<0};
                    if ui.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::Escape))||down(27)||recorder.expired(){self.hotkey_record=None;}else if let Some(binding)=recorder.poll(down){binding.apply(&mut self.prefs);self.hotkey_record=None;}
                }
                ui.weak("Works in the background while a call is active. Hold the recorded hotkey to transmit; releasing it closes the microphone gate.");ui.separator();
                use voice_model::voice_settings::{InputProfile,NoiseSuppression};egui::ComboBox::from_id_salt("processing-profile").selected_text(processing_label(self.prefs.processing.profile)).show_ui(ui,|ui|{for p in [InputProfile::VoiceIsolation,InputProfile::Studio,InputProfile::Custom]{ui.selectable_value(&mut self.prefs.processing.profile,p,processing_label(p));}});
                if self.prefs.processing.profile==InputProfile::Custom{egui::ComboBox::from_id_salt("suppression").selected_text(suppression_label(self.prefs.processing.custom.suppression)).show_ui(ui,|ui|{for p in [NoiseSuppression::Off,NoiseSuppression::RnNoise,NoiseSuppression::WebRtc]{ui.selectable_value(&mut self.prefs.processing.custom.suppression,p,suppression_label(p));}});ui.checkbox(&mut self.prefs.processing.custom.echo_cancellation,"Echo cancellation");ui.checkbox(&mut self.prefs.processing.custom.automatic_gain,"Automatic gain control");let mut threshold=self.prefs.processing.custom.sensitivity_db.unwrap_or(-55);if ui.add(egui::Slider::new(&mut threshold,-80..=0).text("Voice sensitivity dB")).changed(){self.prefs.processing.custom.sensitivity_db=Some(threshold);}}
                ui.separator();ui.strong("Screen share quality");let premium=self.user.as_ref().is_some_and(|u|u.premium_type==2)||self.server.metadata["premium_tier"].as_u64().unwrap_or(0)>=2;
                egui::ComboBox::from_id_salt("share-resolution").selected_text(format!("{}p",self.prefs.screen_height)).show_ui(ui,|ui|{ui.selectable_value(&mut self.prefs.screen_height,720,"720p");ui.add_enabled_ui(premium,|ui|{ui.selectable_value(&mut self.prefs.screen_height,1080,"1080p · Nitro / boosted server");});});
                egui::ComboBox::from_id_salt("share-fps").selected_text(format!("{} fps",self.prefs.screen_fps)).show_ui(ui,|ui|{for fps in [15,30]{ui.selectable_value(&mut self.prefs.screen_fps,fps,format!("{fps} fps"));}ui.add_enabled_ui(premium,|ui|{ui.selectable_value(&mut self.prefs.screen_fps,60,"60 fps · Nitro / boosted server");});});if !premium{self.prefs.screen_height=720;self.prefs.screen_fps=self.prefs.screen_fps.min(30);}ui.weak("This native encoder currently supports up to 1080p60. Higher resolution increases RAM and GPU use. Camera uses the default Windows camera.");
            }
            "Appearance"=>{for theme in preferences::Theme::presets(){let name=theme.name.clone();if ui.selectable_label(self.prefs.theme.name==name,name).clicked(){self.prefs.theme=theme;}}ui.separator();ui.add(egui::Slider::new(&mut self.prefs.zoom,0.75..=1.5).text("UI scale"));ui.add(egui::Slider::new(&mut self.prefs.font_size,12.0..=24.0).text("Chat font size"));ui.checkbox(&mut self.prefs.compact,"Compact mode");ui.checkbox(&mut self.prefs.members,"Show member panel");}
            "Accessibility"=>{ui.checkbox(&mut self.prefs.reduced_motion,"Reduce motion");ui.checkbox(&mut self.prefs.contrast,"High contrast text");ui.checkbox(&mut self.prefs.animations,"Play animated images");ui.add(egui::Slider::new(&mut self.prefs.font_size,12.0..=24.0).text("Text size"));ui.weak("Keyboard navigation uses Tab, Shift+Tab, Enter and Escape. Windows font fallbacks cover decorated channel names.");}
            "Chat"=>{ui.checkbox(&mut self.prefs.compact,"Compact mode");ui.checkbox(&mut self.prefs.images,"Show inline images and GIFs");ui.checkbox(&mut self.prefs.animations,"Autoplay animated images");ui.checkbox(&mut self.prefs.show_usernames,"Show username beside display name");ui.checkbox(&mut self.prefs.character_count,"Show message character count");ui.checkbox(&mut self.prefs.role_colors,"Use server role colors");ui.checkbox(&mut self.prefs.quiet_mentions,"Hide mention highlight backgrounds");}
            "Content & Social"|"Data & Privacy"|"Activity Privacy"|"Language"|"Notifications"=>{
                self.feature_status(ui,"settings");if self.settings_edit.is_object(){match page.as_str(){
                    "Content & Social"=>{toggle(ui,&mut self.settings_edit,"default_guilds_restricted","Disable DMs from new servers by default");choice(ui,&mut self.settings_edit,"explicit_content_filter","Sensitive media filter",&[(0,"Do not scan"),(1,"Scan non-friends"),(2,"Scan all")]);ui.strong("Friend requests from");if !self.settings_edit["friend_source_flags"].is_object(){self.settings_edit["friend_source_flags"]=json!({});}for (key,label) in [("all","Everyone"),("mutual_friends","Friends of friends"),("mutual_guilds","Server members")]{toggle(ui,&mut self.settings_edit["friend_source_flags"],key,label);}},
                    "Data & Privacy"=>{ui.weak("Review Discord's current privacy choices in its secure account flow. Legacy fields unavailable from Discord are not fabricated.");ui.hyperlink_to("Discord privacy settings guide","https://support.discord.com/hc/en-us/articles/217916488-Blocking-Privacy-Settings");for(key,label)in[("detect_platform_accounts","Detect connected platform accounts"),("contact_sync_enabled","Sync mobile contacts"),("allow_accessibility_detection","Allow screen reader detection")]{if self.settings_edit.get(key).is_some(){toggle(ui,&mut self.settings_edit,key,label);}}},
                    "Activity Privacy"=>{toggle(ui,&mut self.settings_edit,"show_current_game","Share current activity");toggle(ui,&mut self.settings_edit,"allow_activity_party_privacy_friends","Allow friends to join activities");toggle(ui,&mut self.settings_edit,"allow_activity_party_privacy_voice_channel","Allow voice channel members to join activities");ui.weak("Eclipse does not scan your running games. This changes the account's activity-sharing setting.");},
                    "Language"=>{language_picker(ui,&mut self.settings_edit,"locale");ui.weak("Eclipse's interface currently uses English.");},
                    "Notifications"=>{ui.checkbox(&mut self.prefs.ui_sounds,"Voice connection, mute and deafen sounds");for(key,label)in[("enable_tts_command","Allow text-to-speech commands"),("render_reactions","Show reactions"),("render_embeds","Show embeds")]{toggle(ui,&mut self.settings_edit,key,label);}ui.weak("Server notification levels are under Server Settings → Notifications. Native Windows toast notifications are not implemented yet. Voice feedback plays locally.");},_=>{}
                }ui.add_space(12.);if ui.add_enabled(!self.feature_pending.contains("save-settings"),primary("Save account settings")).clicked(){let base=self.features.get("settings").cloned().unwrap_or(Value::Null);let changes=delta(&base,&self.settings_edit);self.mutate("save-settings",Method::PATCH,"/users/@me/settings".into(),Some(changes));}self.feature_status(ui,"save-settings");}else if ui.button("Load account settings").clicked(){self.request_feature("settings","/users/@me/settings".into());}
            }
            "Connections"|"Devices"|"Authorized Apps"|"Gift Inventory"|"Server Boost"|"Subscriptions"=>{let key=match page.as_str(){"Connections"=>"connections","Devices"=>"devices","Authorized Apps"=>"authorized-apps","Gift Inventory"=>"inventory","Server Boost"=>"boosts",_=>"subscriptions"};self.feature_status(ui,key);if let Some(data)=self.features.get(key).cloned(){account_cards(ui,&data,&page,self.prefs.developer);}if ui.button("Refresh").clicked(){self.load_settings_section();}ui.add_space(12.);ui.weak("Connection authorization, device revocation and subscription billing use Discord's secure account flow.");ui.hyperlink_to("Manage in Discord","https://discord.com/channels/@me");}
            "Nitro"=>self.nitro_content(ui),
            "Billing"=>{ui.label("Manage payment methods and receipts through Discord's secure checkout.");ui.hyperlink_to("Open Discord billing","https://discord.com/channels/@me");ui.weak("Eclipse never collects card details or purchases automatically.");},
            "Family Center"=>{ui.label("Family links, supervision and age verification must be completed in Discord.");ui.hyperlink_to("Open Family Center help","https://support.discord.com/hc/en-us/categories/15878301948823-Family-Center");},
            "Keybinds"=>{ui.label("Enter · Send message");ui.label("Shift+Enter · New line");ui.label("Ctrl+R · Refresh the selected conversation");ui.label("Ctrl+, · Open User Settings");ui.label("Ctrl+Shift+M · Mute / unmute (while Eclipse is focused)");ui.label("Ctrl+Shift+D · Deafen / undeafen (while Eclipse is focused)");ui.separator();ui.strong("Global Push to Talk");ui.label(hotkey_label(&self.prefs));if ui.button("Change push-to-talk hotkey").clicked(){self.settings_page="Voice & Video".into();self.load_settings_section();}ui.weak("Other global keybinds are not registered. Push to talk works with Eclipse in the background.");},
            "Streamer Mode"=>{ui.checkbox(&mut self.prefs.streamer,"Hide your account name in the account bar");ui.weak("This does not hide content in chat or automatically detect broadcasting software.");},
            "Advanced"=>{ui.checkbox(&mut self.prefs.developer,"Developer mode");ui.weak("User, message and channel context menus include copy-ID actions.");if ui.button("Clear image cache").clicked(){self.images.clear();}if ui.button("Refresh servers and folders").clicked(){if !self.preview{self.send_command(Command::Refresh);}}},
            "Registered Games"=>{ui.label("Automatic game detection is not implemented. Eclipse does not inspect running processes.");ui.checkbox(&mut self.game_activity,"Activity sharing enabled");if ui.button("Save activity sharing").clicked(){self.mutate("save-settings",Method::PATCH,"/users/@me/settings".into(),Some(json!({"show_current_game":self.game_activity})));}self.feature_status(ui,"save-settings");},
            "Plugins"=>self.plugins_settings(ui),
            "Themes"=>self.themes_settings(ui),
            "Performance"=>{
                ui.strong("Choose your balance");ui.weak("Apply a preset, then fine-tune it below.");
                ui.horizontal_wrapped(|ui|{
                    if ui.button("Smooth").clicked(){self.prefs.animation_fps=60;self.prefs.animations=true;self.prefs.images=true;self.prefs.idle_seconds=3;}
                    if ui.button("Balanced").clicked(){self.prefs.animation_fps=30;self.prefs.animations=true;self.prefs.images=true;self.prefs.idle_seconds=5;}
                    if ui.button("Save memory").clicked(){self.prefs.animation_fps=15;self.prefs.animations=false;self.prefs.images=false;self.prefs.members=false;self.prefs.idle_seconds=5;}
                });
                ui.separator();ui.add(egui::Slider::new(&mut self.prefs.animation_fps,5..=60).text("Animation smoothness (fps)"));
                ui.checkbox(&mut self.prefs.animations,"Animate visible images");ui.checkbox(&mut self.prefs.images,"Show images and GIFs in chat");
                ui.weak(format!("Eclipse is using {:.0} MB of physical memory.",self.memory.working_mb));
                if ui.button("Clear image cache").clicked(){self.images.clear();}
                ui.collapsing("Advanced diagnostics",|ui|{
                    ui.label(format!("Private commit: {:.0} MiB",self.memory.private_mb));ui.label(format!("Images: {} / 128 · {:.1} / 48 MiB",self.images.len(),self.images.bytes()as f64/1048576.));
                    ui.label(format!("{} animated · {} sampled · {} static fallbacks",self.images.animated(),self.images.sampled(),self.images.fallbacks()));
                    ui.add(egui::Slider::new(&mut self.prefs.idle_seconds,1..=10).text("Idle refresh interval (seconds)"));
                });ui.weak("Recent conversations stay in a small memory cache. First visits still depend on your connection. Calls and screen sharing use additional memory.");
            }
            _=>{}
        }
            });
    }
    pub(super) fn plugins_settings(&mut self,ui:&mut egui::Ui){
        ui.weak("Native implementations of the requested features. Vencord JavaScript plugins require Discord's web runtime and cannot be loaded here.");ui.add_space(12.);
        for (setting,label) in [(&mut self.prefs.spotify,"Spotify Controls · Windows media session playback"),(&mut self.prefs.volume_booster,"VolumeBooster · user and stream playback up to 1000%"),(&mut self.prefs.silent_typing,"Silent Typing · suppress outgoing typing indicators"),(&mut self.prefs.read_all,"Read All · acknowledge all known server/DM read states"),(&mut self.prefs.message_logger,"Message Logger · retain 200 received deletions / previous edits in RAM"),(&mut self.prefs.click_actions,"MessageClickActions · double-click reply, Ctrl+click copy, Alt+click edit"),(&mut self.prefs.activity_toggle,"GameActivityToggle · button beside mute"),(&mut self.prefs.role_colors,"Role colors"),(&mut self.prefs.character_count,"Message character count")]{ui.checkbox(setting,label);}
        ui.separator();ui.strong("SpotifyCrack: no auto-pause");ui.label("Eclipse never automatically pauses Spotify when you speak in a call.");ui.weak("Premium spoofing and Free Listen Along are not implemented. Playback controls use Spotify's Windows media session and its available capabilities.");
        ui.separator();ui.strong("Local display plugins");let mut remove=None;for(index,plugin)in self.prefs.plugins.iter_mut().enumerate(){ui.horizontal(|ui|{ui.checkbox(&mut plugin.enabled,&plugin.name);if ui.small_button("Remove").clicked(){remove=Some(index);}});ui.weak(&plugin.description);}if let Some(i)=remove{self.prefs.plugins.remove(i);}
        if ui.button("Import Eclipse display plugin (.json)…").clicked(){if let Some(path)=rfd::FileDialog::new().add_filter("Eclipse display plugin",&["json"]).pick_file(){let result=read_bounded(&path,64*1024).and_then(|s|preferences::Plugin::parse(&s));match result{Ok(plugin)if self.prefs.plugins.len()<32=>self.prefs.plugins.push(plugin),Ok(_)=>{self.error=Some("Maximum 32 display plugins.".into());},Err(error)=>self.error=Some(error)}}}ui.weak("Display plugins only replace text locally. They cannot access tokens, execute JavaScript or change outgoing messages.");
    }
    pub(super) fn themes_settings(&mut self,ui:&mut egui::Ui){
        for theme in preferences::Theme::presets(){if ui.selectable_label(self.prefs.theme.name==theme.name,&theme.name).clicked(){self.prefs.theme=theme;}}
        for(label,value)in[("Background",&mut self.prefs.theme.background),("Panels",&mut self.prefs.theme.surface),("Accent",&mut self.prefs.theme.accent),("Text",&mut self.prefs.theme.text)]{ui.horizontal(|ui|{ui.label(label);let mut rgb=preferences::color(value).unwrap_or(Color32::GRAY).to_array()[..3].try_into().unwrap();if ui.color_edit_button_srgb(&mut rgb).changed(){*value=format!("#{:02x}{:02x}{:02x}",rgb[0],rgb[1],rgb[2]);}});}
        if ui.button("Import native JSON / CSS color variables…").clicked(){if let Some(path)=rfd::FileDialog::new().add_filter("Theme",&["json","css"]).pick_file(){match read_bounded(&path,256*1024).and_then(|s|preferences::Theme::import(&s)){Ok(theme)=>self.prefs.theme=theme,Err(error)=>self.error=Some(error)}}}
        if ui.button("Export current theme…").clicked(){if let Some(path)=rfd::FileDialog::new().set_file_name("eclipse-theme.json").save_file(){if std::fs::write(path,serde_json::to_vec_pretty(&self.prefs.theme).unwrap()).is_err(){self.error=Some("Could not export theme.".into());}}}ui.weak("CSS import reads literal #RRGGBB color variables only. CSS layouts, @imports, scripts and arbitrary Vencord themes do not run in a native renderer.");
    }
}
fn device_picker(ui:&mut egui::Ui,id:&str,selected:&mut Option<String>,devices:Option<&[(String,String)]>){let label=selected.as_ref().and_then(|id|devices.and_then(|ds|ds.iter().find(|d|&d.0==id))).map(|d|d.1.as_str()).unwrap_or("System default");egui::ComboBox::from_id_salt(id).width(360.).selected_text(label).show_ui(ui,|ui|{ui.selectable_value(selected,None,"System default");if let Some(devices)=devices{for (id,name)in devices{ui.selectable_value(selected,Some(id.clone()),name);}}});}
fn hotkey_label(p:&Preferences)->String{let key=match p.ptt_key{1=>"Mouse 1".into(),2=>"Mouse 2".into(),4=>"Mouse middle".into(),5=>"Mouse 4".into(),6=>"Mouse 5".into(),16|160|161=>"Shift".into(),17|162|163=>"Ctrl".into(),18|164|165=>"Alt".into(),32=>"Space".into(),9=>"Tab".into(),13=>"Enter".into(),0x70..=0x87=>format!("F{}",p.ptt_key-0x6f),0x30..=0x5a=>char::from_u32(p.ptt_key).unwrap_or('?').to_string(),n=>format!("Key {n}")};format!("{}{}{}{key}",if p.ptt_ctrl{"Ctrl+"}else{""},if p.ptt_shift{"Shift+"}else{""},if p.ptt_alt{"Alt+"}else{""})}
fn read_bounded(path:&std::path::Path,max:u64)->Result<String,String>{if std::fs::metadata(path).map_err(|_|"File unavailable.")?.len()>max{return Err("File exceeds the supported size.".into());}std::fs::read_to_string(path).map_err(|_|"Could not read UTF-8 file.".into())}
fn avatar_data(path:&std::path::Path)->Result<String,String>{use base64::Engine;let meta=std::fs::metadata(path).map_err(|_|"Image unavailable.")?;if meta.len()>8*1024*1024{return Err("Choose a profile image under 8 MiB.".into());}let bytes=std::fs::read(path).map_err(|_|"Image unavailable.")?;let mime=match image::guess_format(&bytes).map_err(|_|"Unsupported image.")?{image::ImageFormat::Png=>"image/png",image::ImageFormat::Jpeg=>"image/jpeg",image::ImageFormat::Gif=>"image/gif",image::ImageFormat::WebP=>"image/webp",_=>return Err("Use PNG, JPEG, GIF or WebP.".into())};Ok(format!("data:{mime};base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes)))}
fn nitro_name(t:u8)->&'static str{match t{1=>"Nitro Classic",2=>"Discord Nitro",3=>"Nitro Basic",_=>"No Nitro subscription reported"}}

fn friendly_name(value:&str)->String{value.split(['_','-']).filter(|s|!s.is_empty()).map(|word|{let mut chars=word.chars();chars.next().map(|first|first.to_uppercase().collect::<String>()+&chars.as_str().to_lowercase()).unwrap_or_default()}).collect::<Vec<_>>().join(" ")}
fn json_cards(ui:&mut egui::Ui,data:&Value){account_cards(ui,data,"Server configuration",false);}
fn account_cards(ui:&mut egui::Ui,data:&Value,page:&str,developer:bool){
    let items:Vec<&Value>=data.as_array().or_else(||data["user_sessions"].as_array()).map(|items|items.iter().take(100).collect()).unwrap_or_else(||vec![data]);
    if items.is_empty(){ui.weak("Nothing here yet.");}
    for(index,item)in items.iter().enumerate(){ui.push_id(index,|ui|{egui::Frame::NONE.fill(CARD).corner_radius(12).inner_margin(16).show(ui,|ui|{
        let title=item["application"]["name"].as_str().or_else(||item["guild"]["name"].as_str()).or_else(||item["name"].as_str()).or_else(||item["client_info"]["os"].as_str()).or_else(||item["subscription_plan"]["name"].as_str()).unwrap_or(match page{"Devices"=>"Signed-in device","Subscriptions"=>"Discord subscription","Server Boost"=>"Server boost","Gift Inventory"=>"Gift","Authorized Apps"=>"Connected app",_=>"Configuration"});
        ui.strong(title);
        if let Some(service)=item["type"].as_str(){ui.weak(friendly_name(service));}
        if page=="Devices"{for key in ["platform","location"]{if let Some(value)=item["client_info"][key].as_str(){ui.label(friendly_name(value));}}}
        for(key,label)in[("verified","Verified"),("current","This device"),("active","Active"),("cancel_at_period_end","Cancels at the end of this period")]{if item[key].as_bool()==Some(true){ui.label(format!("✓ {label}"));}}
        if let Some(status)=item["status"].as_str(){ui.label(friendly_name(status));}
        for(key,label)in[("created_at","Added"),("expires_at","Expires"),("current_period_end","Current period ends"),("approx_last_used_time","Last used")]{if let Some(date)=item[key].as_str(){ui.weak(format!("{label} {}",date.get(..10).unwrap_or(date)));}}
        if item["available"].as_bool()==Some(false){ui.weak("Unavailable");}
        if developer{ui.collapsing("Technical details",|ui|{for key in ["id","type","status"]{if let Some(value)=item.get(key){ui.label(format!("{key}: {value}"));}}});}
    });});ui.add_space(8.);}
}
fn language_picker(ui:&mut egui::Ui,value:&mut Value,key:&str){let mut selected=value[key].as_str().unwrap_or("en-US").to_owned();let choices=[("en-US","English (United States)"),("en-GB","English (United Kingdom)"),("fr","Français"),("de","Deutsch"),("es-ES","Español"),("pt-BR","Português (Brasil)"),("ja","日本語"),("ko","한국어"),("zh-CN","简体中文"),("ru","Русский"),("uk","Українська"),("it","Italiano"),("nl","Nederlands"),("pl","Polski"),("tr","Türkçe")];ui.strong("Language");egui::ComboBox::from_id_salt(key).width(ui.available_width().min(340.)).selected_text(choices.iter().find(|(code,_)|*code==selected).map(|(_,label)|*label).unwrap_or("Current account language")).show_ui(ui,|ui|{for(code,label)in choices{ui.selectable_value(&mut selected,code.into(),label);}});value[key]=json!(selected);}
fn processing_label(p:voice_model::voice_settings::InputProfile)->&'static str{use voice_model::voice_settings::InputProfile::*;match p{VoiceIsolation=>"Voice isolation · reduce background noise",Studio=>"Studio · natural sound",Custom=>"Custom processing"}}
fn suppression_label(p:voice_model::voice_settings::NoiseSuppression)->&'static str{use voice_model::voice_settings::NoiseSuppression::*;match p{Off=>"Off",RnNoise=>"Enhanced noise reduction",WebRtc=>"Standard noise reduction"}}
fn audit_action(n:u64)->&'static str{match n{1=>"Server updated",10=>"Channel created",11=>"Channel updated",12=>"Channel deleted",20=>"Member kicked",22=>"Member banned",23=>"Member unbanned",24=>"Member updated",25=>"Member roles updated",30=>"Role created",31=>"Role updated",32=>"Role deleted",40=>"Invite created",41=>"Invite updated",42=>"Invite deleted",60=>"Emoji created",61=>"Emoji updated",62=>"Emoji deleted",72=>"Message deleted",73=>"Messages deleted",74=>"Message pinned",75=>"Message unpinned",_=>"Server activity"}}
