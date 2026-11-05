use super::*;
use crate::{community::{self,Member,Role,Relationship},preferences::{self,Preferences,USER_CATEGORIES}};
use reqwest::Method;
use serde_json::{json,Value};
#[path="home.rs"] mod home;
#[path="server_settings.rs"] mod server_settings;
#[path="interactions.rs"] mod interactions;
#[path="profile_popout.rs"] mod profile_popout;
#[path="dm_picker.rs"] mod dm_picker;
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
            "connections-write"=>{self.request_feature("connections","/users/@me/connections".into());return;},
            "authorized-apps-write"=>{self.request_feature("authorized-apps","/oauth2/tokens".into());return;},
            "read-all"=>{self.unread.clear();self.features.insert(key,json!({"saved":true}));return;},
            "relationship"=>{self.request_feature("friends","/users/@me/relationships".into());return;},
            "save-notifications"=>{let key=format!("server:{}:Notifications",self.server.id);self.features.insert(key,data);return;},
            "server-write"=>{self.load_server_page();return;},
            "save-channel"|"create-channel"=>{if let Some(g)=self.guild.clone(){self.send_command(Command::Channels(g));}return;},
            _=>{}
        }
        if let Some(guild)=key.strip_prefix("guild:"){if guild==self.server.id{self.server.metadata=data.clone();self.server_edit=data.clone();if let Some(roles)=data.get("roles"){if let Ok(roles)=serde_json::from_value(roles.clone()){self.server.roles=roles;}}}return;}
        if let Some(guild)=key.strip_prefix("roles:"){if guild==self.server.id{if let Ok(roles)=serde_json::from_value(data){self.server.roles=roles;}}return;}
        if let Some(guild)=key.strip_prefix("me-member:"){if guild==self.server.id{if let Ok(member)=serde_json::from_value(data){self.server.insert(member);}}return;}
        if let Some(guild)=key.strip_prefix("members:"){if guild==self.server.id{if let Ok(members)=serde_json::from_value::<Vec<Member>>(data){self.server.has_more=members.len()==100;self.server.cursor=members.last().map(|m|m.user.id.clone());for member in members{self.server.insert(member);}}}return;}
        if key.starts_with("profile:") { let full=self.full_profile.as_ref().is_some_and(|u|profile_key_for(&u.id,self.full_profile_guild.as_deref())==key); let popout=self.profile.as_ref().is_some_and(|u|self.profile_key(&u.id)==key); if !popout&&!full{return;} if full{if let Some(user)=self.full_profile.clone(){self.full_profile=Some(crate::identity::merged(&user,&data));}} if let Some(user)=self.profile.clone().filter(|_|popout){self.profile=Some(crate::identity::merged(&user,&data));self.profile_patch("USER_UPDATE",&data["user"]);if self.profile_guild.as_deref()==Some(&self.server.id){if let Some(member)=self.server.members.get_mut(&user.id){if data["guild_member"].get("avatar_decoration_data").is_some(){member.avatar_decoration_data=data["guild_member"]["avatar_decoration_data"].as_object().map(|_|data["guild_member"]["avatar_decoration_data"].clone());}}}} }
        if self.features.len()>40{self.features.retain(|key,_|matches!(key.as_str(),"settings"|"my-profile"));}
        self.features.insert(key,data);
    }
    /// Keeps every server's custom emojis (from READY, GUILD_CREATE and GUILD_EMOJIS_UPDATE).
    fn remember_emojis(&mut self,kind:&str,data:&Value){
        let mut keep=|id:Option<&str>,name:Option<&str>,emojis:&Value|{
            let (Some(id),Some(list))=(id,emojis.as_array()) else{return};
            let emojis:Vec<crate::media_picker::CustomEmoji>=list.iter().filter_map(|e|serde_json::from_value(e.clone()).ok()).take(500).collect();
            let name=name.map(str::to_owned).or_else(||self.server_emojis.get(id).map(|(n,_)|n.clone())).unwrap_or_else(||"Server".into());
            if self.server_emojis.len()<300||self.server_emojis.contains_key(id){self.server_emojis.insert(id.to_owned(),(name,emojis));}
        };
        match kind{
            "READY"=>for guild in data["guilds"].as_array().into_iter().flatten().take(300){keep(guild["id"].as_str(),guild["name"].as_str().or(guild["properties"]["name"].as_str()),&guild["emojis"]);},
            "GUILD_CREATE"=>keep(data["id"].as_str(),data["name"].as_str().or(data["properties"]["name"].as_str()),&data["emojis"]),
            "GUILD_EMOJIS_UPDATE"=>keep(data["guild_id"].as_str(),None,&data["emojis"]),
            "GUILD_DELETE"=>{if let Some(id)=data["id"].as_str(){self.server_emojis.remove(id);}},
            _=>{}
        }
    }
    /// With any Nitro plan, emojis from all your other servers work everywhere (servers, DMs,
    /// group chats); without Nitro only the current server's own emojis are offered.
    pub(super) fn nitro_emojis(&self)->Vec<(String,Vec<crate::media_picker::CustomEmoji>)>{
        if !self.user.as_ref().is_some_and(|u|u.premium_type>0){return vec![];}
        let mut servers:Vec<_>=self.server_emojis.iter().filter(|(id,(_,emojis))|self.guild.as_deref()!=Some(id.as_str())&&!emojis.is_empty()).map(|(_,(name,emojis))|(name.clone(),emojis.clone())).collect();
        servers.sort_by(|a,b|a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        servers
    }
    pub(super) fn account_event(&mut self,kind:&str,data:&Value){
        self.voice.ingest(kind,data);
        self.server.ingest(kind,data);
        if matches!(kind,"CHANNEL_CREATE"|"CHANNEL_UPDATE"|"CHANNEL_DELETE"|"GUILD_DELETE"){if let Some(id)=data["guild_id"].as_str().or_else(||if kind=="GUILD_DELETE"{data["id"].as_str()}else{None}){self.navigation.invalidate_channels(id);}}
        // Live channel changes in the open server (for example a bot's new voice channel).
        if matches!(kind,"CHANNEL_CREATE"|"CHANNEL_UPDATE"|"CHANNEL_DELETE")&&data["guild_id"].as_str().is_some_and(|g|self.guild.as_deref()==Some(g)){
            if let Ok(channel)=serde_json::from_value::<Channel>(data.clone()){
                self.channels.retain(|c|c.id!=channel.id);
                if kind!="CHANNEL_DELETE"&&self.channels.len()<1000{self.channels.push(channel);}
                crate::navigation::sort_channels(&mut self.channels);
                if let Some(guild)=self.guild.clone(){self.navigation.save_channels(&guild,&self.channels);}
            }
        }
        self.remember_emojis(kind,data);
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
    pub(super) fn navigate_home(&mut self,home:Home){self.remember_conversation();self.images.release_conversation();self.home=home;self.profile=None;self.guild=None;self.channels=self.dms.clone();self.channel=None;self.messages.clear();self.search.clear();match home{Home::Friends=>self.request_feature("friends","/users/@me/relationships".into()),_=>{}}}
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
use crate::settings_layout as sl;
fn text(ui:&mut egui::Ui,value:&mut Value,key:&str,label:&str,multiline:bool){sl::label(ui,label);let mut s=value[key].as_str().unwrap_or("").to_owned();let response=if multiline{ui.add(egui::TextEdit::multiline(&mut s).desired_rows(3).desired_width(f32::INFINITY).margin(egui::Margin::symmetric(10,8)))}else{ui.add(egui::TextEdit::singleline(&mut s).desired_width(f32::INFINITY).margin(egui::Margin::symmetric(10,8)))};if response.changed(){value[key]=json!(s);}ui.add_space(10.);}
fn toggle(ui:&mut egui::Ui,value:&mut Value,key:&str,label:&str){toggle_desc(ui,value,key,label,"");}
fn toggle_desc(ui:&mut egui::Ui,value:&mut Value,key:&str,label:&str,description:&str){let mut v=value[key].as_bool().unwrap_or(false);if sl::toggle_row(ui,label,description,&mut v){value[key]=json!(v);}}
fn number(ui:&mut egui::Ui,value:&mut Value,key:&str,label:&str,range:std::ops::RangeInclusive<i64>){sl::label(ui,label);let mut v=value[key].as_i64().unwrap_or(*range.start());if sl::slider(ui,&mut v,range,|v|format!("{v:.0}")).changed(){value[key]=json!(v);}}
fn choice(ui:&mut egui::Ui,value:&mut Value,key:&str,label:&str,choices:&[(i64,&str)]){sl::label(ui,label);let mut v=value[key].as_i64().unwrap_or(0);egui::ComboBox::from_id_salt(key).width(ui.available_width().min(380.)).selected_text(choices.iter().find(|c|c.0==v).map(|c|c.1).unwrap_or("Unspecified")).show_ui(ui,|ui|{for (n,label) in choices{ui.selectable_value(&mut v,*n,*label);}});ui.add_space(10.);if value[key].as_i64()!=Some(v){value[key]=json!(v);}}
fn delta(old:&Value,new:&Value)->Value{let mut result=json!({});if let Some(o)=new.as_object(){for(k,v)in o{if old.get(k)!=Some(v){result[k]=v.clone();}}}result}
fn heading(ui:&mut egui::Ui,title:&str,description:&str){sl::page_heading(ui,title,description);}
fn toggle_pref(ui:&mut egui::Ui,on:&mut bool,title:&str,description:&str){sl::toggle_row(ui,title,description,on);}
fn link(ui:&mut egui::Ui,text:&str,url:&str){ui.hyperlink_to(RichText::new(format!("{text} ↗")).size(13.5),url);}

impl Eclipse {
    pub(super) fn settings_window(&mut self,ctx:&egui::Context){
        if !self.settings{return;}
        sl::set_accent(ctx,self.accent());
        let close=sl::shell(ctx,"user-settings","User Settings","Personalize your Eclipse experience",|ui,size|{
            ui.horizontal_top(|ui|{
                ui.spacing_mut().item_spacing.x=0.;
                self.settings_sidebar(ui,size);
                egui::Frame::NONE.inner_margin(egui::Margin{left:30,right:18,top:20,bottom:8}).show(ui,|ui|{ui.vertical(|ui|{ui.set_width(size.x-268.);
                    egui::ScrollArea::vertical().id_salt(("settings-body",&self.settings_page)).max_height(size.y-28.).auto_shrink([false,false]).show(ui,|ui|{
                        ui.set_max_width((size.x-280.).min(720.));self.settings_body(ui);ui.add_space(24.);
                    });
                });});
            });
        });
        if close{self.settings=false;self.hotkey_record=None;}
    }
    fn settings_sidebar(&mut self,ui:&mut egui::Ui,size:Vec2){
        egui::Frame::NONE.fill(RAIL).inner_margin(egui::Margin{left:12,right:12,top:14,bottom:12}).show(ui,|ui|{ui.vertical(|ui|{ui.set_width(196.);ui.set_height(size.y-26.);
            ui.scope(|ui|{sl::style(ui);ui.visuals_mut().extreme_bg_color=Color32::from_gray(30);
                ui.add(egui::TextEdit::singleline(&mut self.settings_search).hint_text("Search settings").desired_width(196.).margin(egui::Margin::symmetric(10,7)));});
            ui.add_space(4.);
            egui::ScrollArea::vertical().id_salt("settings-nav").max_height(size.y-118.).show(ui,|ui|{
                let query=self.settings_search.to_lowercase();
                for(category,pages)in USER_CATEGORIES{let visible:Vec<_>=pages.iter().filter(|p|p.to_lowercase().contains(&query)||category.to_lowercase().contains(&query)).collect();if visible.is_empty(){continue;}
                    ui.add_space(14.);ui.label(RichText::new(*category).size(10.5).strong().color(MUTED));ui.add_space(2.);
                    for page in visible{let selected=self.settings_page==**page;if crate::widgets::nav_row(ui,page,selected,if selected{TEXT}else{MUTED}).clicked(){self.settings_page=(**page).into();self.hotkey_record=None;self.load_settings_section();}}
                }
            });
            ui.add_space(6.);let (line,_)=ui.allocate_exact_size(Vec2::new(196.,1.),egui::Sense::hover());ui.painter().rect_filled(line,0,Color32::from_gray(40));ui.add_space(6.);
            if crate::widgets::nav_row(ui,"Log out",false,Color32::from_rgb(242,95,100)).clicked(){self.log_out();self.settings=false;}
        });});
    }
    pub(super) fn load_settings_section(&mut self){
        match self.settings_page.as_str(){
            "Account & Profile"=>self.load_my_profile(),
            "Connections"=>self.request_feature("connections","/users/@me/connections".into()),
            "Devices"=>self.request_feature("devices","/auth/sessions".into()),
            "Authorized Apps"=>self.request_feature("authorized-apps","/oauth2/tokens".into()),
            "Voice & Video"=>{self.audio_devices=discord_voice::audio::devices().ok();},
            _=>{}
        }
    }
    pub(super) fn settings_error(&mut self,ui:&mut egui::Ui){
        if let Some(error)=self.error.clone(){egui::Frame::NONE.fill(Color32::from_rgb(58,26,30)).stroke(Stroke::new(1_f32,Color32::from_rgb(130,48,56))).corner_radius(8).inner_margin(10).show(ui,|ui|{ui.set_width(ui.available_width());ui.horizontal_wrapped(|ui|{ui.colored_label(Color32::from_rgb(255,190,196),error);if ui.small_button("Dismiss").clicked(){self.error=None;}});});ui.add_space(12.);}
    }
    pub(super) fn settings_body(&mut self,ui:&mut egui::Ui){
        sl::style(ui);self.settings_error(ui);
        let page=self.settings_page.clone();heading(ui,&page,sl::description(&page));
        let width=ui.available_width();let half=((width-20.)/2.).max(160.);
        match page.as_str(){
            "Account & Profile"=>{
                if let Some(user)=self.user.clone(){
                    egui::Frame::NONE.fill(Color32::from_gray(30)).corner_radius(12).inner_margin(16).show(ui,|ui|{ui.set_width(ui.available_width());ui.horizontal(|ui|{self.user_avatar(ui,&user,None,64.);ui.add_space(6.);ui.vertical(|ui|{ui.add_space(4.);ui.label(RichText::new(user.name()).size(18.).strong().color(TEXT));ui.label(RichText::new(format!("@{}",user.username)).size(13.).color(MUTED));ui.label(RichText::new(nitro_name(user.premium_type)).size(12.).color(MUTED));});});});
                    ui.add_space(20.);
                }
                self.feature_status(ui,"my-profile");
                if self.account_edit.is_object(){
                    text(ui,&mut self.account_edit,"global_name","Display name",false);text(ui,&mut self.account_edit,"pronouns","Pronouns",false);text(ui,&mut self.account_edit,"bio","About me",true);
                    sl::label(ui,"Profile color");let mut color=self.account_edit["accent_color"].as_u64().unwrap_or(4306893)as u32;let mut rgb=[(color>>16)as u8,(color>>8)as u8,color as u8];
                    ui.horizontal(|ui|{ui.spacing_mut().interact_size=Vec2::new(52.,30.);if ui.color_edit_button_srgb(&mut rgb).changed(){color=((rgb[0]as u32)<<16)|((rgb[1]as u32)<<8)|(rgb[2]as u32);self.account_edit["accent_color"]=json!(color);}ui.label(RichText::new(format!("#{:02X}{:02X}{:02X}",rgb[0],rgb[1],rgb[2])).monospace().color(MUTED));});
                    ui.add_space(10.);sl::label(ui,"Avatar");
                    ui.horizontal(|ui|{if ui.button("Change avatar…").clicked(){if let Some(path)=rfd::FileDialog::new().add_filter("Image",&["png","jpg","jpeg","gif","webp"]).pick_file(){match avatar_data(&path){Ok(data)=>self.account_edit["avatar"]=json!(data),Err(error)=>{self.feature_errors.insert("save-profile".into(),error);}}}}if ui.button("Remove avatar").clicked(){self.account_edit["avatar"]=Value::Null;}});
                    ui.add_space(16.);
                    if sl::primary(ui,"Save profile",!self.feature_pending.contains("save-profile")).clicked(){let body=delta(&self.features.get("my-profile").map(profile_fields).unwrap_or(Value::Null),&self.account_edit);if body.as_object().is_some_and(|o|!o.is_empty()){self.mutate("save-profile",Method::PATCH,"/users/@me".into(),Some(body));}}
                }
                self.feature_status(ui,"save-profile");sl::divider(ui);
                sl::section(ui,"Password and Authentication");sl::help(ui,"Security verification, password changes, passkeys, two-factor authentication and account deletion use Discord's own account flow.");link(ui,"Open Discord account security","https://discord.com/channels/@me");
            }
            "Voice & Video"=>{
                ui.horizontal_top(|ui|{ui.spacing_mut().item_spacing.x=20.;
                    ui.vertical(|ui|{ui.set_width(half);sl::section(ui,"Microphone");device_picker(ui,"voice-input",&mut self.prefs.input,self.audio_devices.as_ref().map(|d|d.inputs.as_slice()),half);ui.add_space(12.);sl::label(ui,"Microphone volume");sl::slider(ui,&mut self.prefs.input_gain,0..=200,sl::percent);});
                    ui.vertical(|ui|{ui.set_width(half);sl::section(ui,"Speaker");device_picker(ui,"voice-output",&mut self.prefs.output,self.audio_devices.as_ref().map(|d|d.outputs.as_slice()),half);ui.add_space(12.);sl::label(ui,"Speaker volume");sl::slider(ui,&mut self.prefs.output_gain,0..=200,sl::percent);});
                });
                ui.add_space(4.);if ui.button("⟳  Refresh devices").clicked(){self.audio_devices=discord_voice::audio::devices().ok();}
                sl::divider(ui);
                sl::section(ui,"Input Mode");
                if sl::radio_row(ui,!self.prefs.push_to_talk,"Voice Activity","Your microphone opens automatically when you speak."){self.prefs.push_to_talk=false;}
                if sl::radio_row(ui,self.prefs.push_to_talk,"Push to Talk","Hold a shortcut to transmit. Works while Eclipse is in the background."){self.prefs.push_to_talk=true;}
                ui.add_space(10.);sl::label(ui,"Push to talk shortcut");
                ui.horizontal(|ui|{for key in hotkey_label(&self.prefs).split('+'){sl::key_pill(ui,key);}ui.add_space(6.);let record=ui.button(if self.hotkey_record.is_some(){"Recording… Esc cancels"}else{"Record shortcut"});if record.clicked()&&self.hotkey_record.is_none(){record.surrender_focus();self.hotkey_record=Some(crate::hotkey::Recorder::new());}});
                if let Some(recorder)=&mut self.hotkey_record{ui.colored_label(sl::accent(ui.ctx()),"Press and release a key or chord. Ctrl, Shift and Alt also work on their own.");ui.ctx().request_repaint_after(Duration::from_millis(16));
                    let down=|k|unsafe{windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(k)<0};
                    if ui.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::Escape))||down(27)||recorder.expired(){self.hotkey_record=None;}else if let Some(binding)=recorder.poll(down){binding.apply(&mut self.prefs);self.hotkey_record=None;}
                }
                sl::help(ui,"Hold the shortcut to transmit; releasing it closes the microphone gate.");
                sl::divider(ui);
                use voice_model::voice_settings::{InputProfile,NoiseSuppression};
                sl::section(ui,"Input Profile");
                for(profile,title,description)in[(InputProfile::VoiceIsolation,"Voice Isolation","Just your voice: Eclipse cuts through background noise."),(InputProfile::Studio,"Studio","Natural sound with minimal processing."),(InputProfile::Custom,"Custom","Advanced mode: choose each processing step yourself.")]{if sl::radio_row(ui,self.prefs.processing.profile==profile,title,description){self.prefs.processing.profile=profile;}}
                if self.prefs.processing.profile==InputProfile::Custom{
                    ui.add_space(14.);sl::label(ui,"Noise suppression");
                    egui::ComboBox::from_id_salt("suppression").width(half).selected_text(suppression_label(self.prefs.processing.custom.suppression)).show_ui(ui,|ui|{for p in [NoiseSuppression::Off,NoiseSuppression::RnNoise,NoiseSuppression::WebRtc]{ui.selectable_value(&mut self.prefs.processing.custom.suppression,p,suppression_label(p));}});
                    ui.add_space(14.);
                    toggle_pref(ui,&mut self.prefs.processing.custom.echo_cancellation,"Echo Cancellation","Stops people hearing themselves through your speakers.");
                    toggle_pref(ui,&mut self.prefs.processing.custom.automatic_gain,"Automatic Gain Control","Keeps your volume steady as you move closer to or further from the mic.");
                    sl::label(ui,"Voice sensitivity");let mut threshold=self.prefs.processing.custom.sensitivity_db.unwrap_or(-55);if sl::slider(ui,&mut threshold,-80..=0,|v|format!("{v:.0} dB")).changed(){self.prefs.processing.custom.sensitivity_db=Some(threshold);}
                    sl::help(ui,"Sound quieter than this level is not transmitted.");
                }
                sl::divider(ui);
                sl::section(ui,"Screen Share Quality");let premium=self.user.as_ref().is_some_and(|u|u.premium_type==2)||self.server.metadata["premium_tier"].as_u64().unwrap_or(0)>=2;
                ui.horizontal_top(|ui|{ui.spacing_mut().item_spacing.x=20.;
                    ui.vertical(|ui|{ui.set_width(half);sl::label(ui,"Resolution");egui::ComboBox::from_id_salt("share-resolution").width(half).selected_text(format!("{}p",self.prefs.screen_height)).show_ui(ui,|ui|{ui.selectable_value(&mut self.prefs.screen_height,720,"720p");ui.add_enabled_ui(premium,|ui|{ui.selectable_value(&mut self.prefs.screen_height,1080,"1080p · Nitro / boosted server");});});});
                    ui.vertical(|ui|{ui.set_width(half);sl::label(ui,"Frame rate");egui::ComboBox::from_id_salt("share-fps").width(half).selected_text(format!("{} fps",self.prefs.screen_fps)).show_ui(ui,|ui|{for fps in [15,30]{ui.selectable_value(&mut self.prefs.screen_fps,fps,format!("{fps} fps"));}ui.add_enabled_ui(premium,|ui|{ui.selectable_value(&mut self.prefs.screen_fps,60,"60 fps · Nitro / boosted server");});});});
                });
                if !premium{self.prefs.screen_height=720;self.prefs.screen_fps=self.prefs.screen_fps.min(30);}
                ui.add_space(4.);sl::help(ui,"The native encoder supports up to 1080p60. Higher resolution increases RAM and GPU use. Camera uses the default Windows camera.");
            }
            "Appearance"=>{
                sl::section(ui,"Theme");ui.add_space(4.);self.theme_swatches(ui);sl::divider(ui);
                sl::section(ui,"Scaling");ui.add_space(4.);
                sl::label(ui,"UI scale");sl::slider(ui,&mut self.prefs.zoom,0.75..=1.5,|v|format!("{:.0}%",v*100.));ui.add_space(6.);
                sl::label(ui,"Chat font size");sl::slider(ui,&mut self.prefs.font_size,12.0..=24.0,|v|format!("{v:.0}px"));sl::divider(ui);
                toggle_pref(ui,&mut self.prefs.compact,"Compact Mode","Fit more messages on screen by zooming the interface out.");
                toggle_pref(ui,&mut self.prefs.members,"Show Member List","Display the member panel beside server channels.");
            }
            "Accessibility"=>{
                toggle_pref(ui,&mut self.prefs.reduced_motion,"Reduce Motion","Tone down animations and transitions across the app.");
                toggle_pref(ui,&mut self.prefs.contrast,"High Contrast Text","Brighten muted text so it is easier to read.");
                toggle_pref(ui,&mut self.prefs.animations,"Play Animated Images","Animate GIFs, animated emoji and avatars.");
                sl::label(ui,"Text size");sl::slider(ui,&mut self.prefs.font_size,12.0..=24.0,|v|format!("{v:.0}px"));
                ui.add_space(8.);sl::help(ui,"Keyboard navigation uses Tab, Shift+Tab, Enter and Escape. Windows font fallbacks cover decorated channel names.");
            }
            "Chat"=>{
                sl::section(ui,"Display");ui.add_space(4.);
                toggle_pref(ui,&mut self.prefs.compact,"Compact Mode","Fit more messages on screen.");
                toggle_pref(ui,&mut self.prefs.show_usernames,"Show Usernames","Show the @username beside display names.");
                toggle_pref(ui,&mut self.prefs.role_colors,"Use Role Colors","Color names with each member's highest server role.");
                toggle_pref(ui,&mut self.prefs.quiet_mentions,"Hide Mention Highlights","Remove the highlighted background from messages that mention you.");
                toggle_pref(ui,&mut self.prefs.character_count,"Show Character Count","Display how many characters are left while typing.");
                ui.add_space(6.);sl::section(ui,"Media");ui.add_space(4.);
                toggle_pref(ui,&mut self.prefs.images,"Show Images and GIFs","Display attachments and links inline in chat.");
                toggle_pref(ui,&mut self.prefs.animations,"Autoplay Animated Images","Play GIFs and animated emoji automatically.");
            }
            "Content & Social"|"Data & Privacy"|"Activity Privacy"|"Language"|"Notifications"=>{
                self.feature_status(ui,"settings");if self.settings_edit.is_object(){match page.as_str(){
                    "Content & Social"=>{
                        sl::section(ui,"Direct Messages");ui.add_space(4.);
                        toggle_desc(ui,&mut self.settings_edit,"default_guilds_restricted","Restrict DMs From New Servers","Members of servers you join later can't message you unless you allow it.");
                        choice(ui,&mut self.settings_edit,"explicit_content_filter","Sensitive media filter",&[(0,"Do not scan"),(1,"Scan non-friends"),(2,"Scan all")]);
                        sl::divider(ui);sl::section(ui,"Who Can Send You Friend Requests");ui.add_space(4.);
                        if !self.settings_edit["friend_source_flags"].is_object(){self.settings_edit["friend_source_flags"]=json!({});}
                        for (key,label) in [("all","Everyone"),("mutual_friends","Friends of Friends"),("mutual_guilds","Server Members")]{toggle(ui,&mut self.settings_edit["friend_source_flags"],key,label);}
                    },
                    "Data & Privacy"=>{
                        for(key,label,description)in[("detect_platform_accounts","Detect Platform Accounts","Suggest connecting accounts Discord detects on this device."),("contact_sync_enabled","Sync Mobile Contacts","Let friends find you from their phone contacts."),("allow_accessibility_detection","Allow Screen Reader Detection","Lets Discord know when you use a screen reader.")]{if self.settings_edit.get(key).is_some(){toggle_desc(ui,&mut self.settings_edit,key,label,description);}}
                        sl::help(ui,"Review Discord's current privacy choices in its secure account flow. Fields Discord does not report are not shown.");link(ui,"Discord privacy settings guide","https://support.discord.com/hc/en-us/articles/217916488-Blocking-Privacy-Settings");
                    },
                    "Activity Privacy"=>{
                        toggle_desc(ui,&mut self.settings_edit,"show_current_game","Share Your Activity","Show what you are playing or listening to as your status.");
                        toggle_desc(ui,&mut self.settings_edit,"allow_activity_party_privacy_friends","Friends Can Join","Allow friends to join your activities.");
                        toggle_desc(ui,&mut self.settings_edit,"allow_activity_party_privacy_voice_channel","Voice Channel Members Can Join","Allow people in your voice channel to join your activities.");
                        sl::help(ui,"Eclipse does not scan your running games. These change your account's activity-sharing settings.");
                    },
                    "Language"=>{language_picker(ui,&mut self.settings_edit,"locale");ui.add_space(6.);sl::help(ui,"Eclipse's interface currently uses English.");},
                    "Notifications"=>{
                        toggle_pref(ui,&mut self.prefs.ui_sounds,"Voice Sounds","Play sounds when you join, leave, mute or deafen. Plays locally.");
                        for(key,label,description)in[("enable_tts_command","Text-to-Speech","Allow /tts messages to be read aloud."),("render_reactions","Show Reactions","Display emoji reactions under messages."),("render_embeds","Show Embeds","Show link previews and rich embeds.")]{toggle_desc(ui,&mut self.settings_edit,key,label,description);}
                        sl::help(ui,"Server notification levels are under Server Settings → Notifications. Native Windows toast notifications are not implemented yet.");
                    },_=>{}
                }
                ui.add_space(14.);if sl::primary(ui,"Save account settings",!self.feature_pending.contains("save-settings")).clicked(){let base=self.features.get("settings").cloned().unwrap_or(Value::Null);let changes=delta(&base,&self.settings_edit);self.mutate("save-settings",Method::PATCH,"/users/@me/settings".into(),Some(changes));}self.feature_status(ui,"save-settings");
                }else if ui.button("Load account settings").clicked(){self.request_feature("settings","/users/@me/settings".into());}
            }
            "Connections"|"Devices"|"Authorized Apps"=>self.account_page(ui,&page),
            "Family Center"=>{sl::section(ui,"Family Center");sl::help(ui,"Family links, supervision and age verification must be completed in Discord.");link(ui,"Open Family Center help","https://support.discord.com/hc/en-us/categories/15878301948823-Family-Center");},
            "Keybinds"=>{
                sl::section(ui,"Messages");ui.add_space(4.);
                for(action,keys)in[("Send message","Enter"),("New line","Shift+Enter"),("Refresh conversation","Ctrl+R"),("Open User Settings","Ctrl+,")]{sl::keybind_row(ui,action,keys);}
                ui.add_space(10.);sl::section(ui,"Voice");ui.add_space(4.);
                for(action,keys)in[("Mute / unmute (Eclipse focused)","Ctrl+Shift+M"),("Deafen / undeafen (Eclipse focused)","Ctrl+Shift+D")]{sl::keybind_row(ui,action,keys);}
                sl::keybind_row(ui,"Push to talk (global)",&hotkey_label(&self.prefs));
                ui.add_space(8.);if ui.button("Change push-to-talk shortcut").clicked(){self.settings_page="Voice & Video".into();self.load_settings_section();}
                sl::help(ui,"Other global keybinds are not registered. Push to talk works with Eclipse in the background.");
            }
            "Streamer Mode"=>{toggle_pref(ui,&mut self.prefs.streamer,"Hide Personal Information","Hide your account name in the account bar.");sl::help(ui,"This does not hide content in chat or automatically detect broadcasting software.");},
            "Advanced"=>{
                toggle_pref(ui,&mut self.prefs.developer,"Developer Mode","Adds Copy ID actions to user, message and channel menus.");
                sl::section(ui,"Maintenance");ui.add_space(4.);
                ui.horizontal(|ui|{if ui.button("Clear image cache").clicked(){self.images.clear();}if ui.button("Refresh servers and folders").clicked()&&!self.preview{self.send_command(Command::Refresh);}});
            }
            "Registered Games"=>{
                toggle_pref(ui,&mut self.game_activity,"Share Activity Status","Display the game you are playing as your status.");
                if sl::primary(ui,"Save activity sharing",true).clicked(){self.mutate("save-settings",Method::PATCH,"/users/@me/settings".into(),Some(json!({"show_current_game":self.game_activity})));}self.feature_status(ui,"save-settings");
                ui.add_space(6.);sl::help(ui,"Automatic game detection is not implemented. Eclipse does not inspect running processes.");
            }
            "Plugins"=>self.plugins_settings(ui),
            "Themes"=>self.themes_settings(ui),
            "Performance"=>{
                sl::section(ui,"Choose Your Balance");sl::help(ui,"Apply a preset, then fine-tune it below.");ui.add_space(6.);
                let p=&self.prefs;let smooth=p.animation_fps==60&&p.animations&&p.images&&p.idle_seconds==3;let balanced=p.animation_fps==30&&p.animations&&p.images&&p.idle_seconds==5;let saver=p.animation_fps==15&&!p.animations&&!p.images;
                let card=((width-24.)/3.).max(120.);
                ui.horizontal(|ui|{ui.spacing_mut().item_spacing.x=12.;
                    if sl::option_card(ui,card,smooth,"Smooth","60 fps, all media"){self.prefs.animation_fps=60;self.prefs.animations=true;self.prefs.images=true;self.prefs.idle_seconds=3;}
                    if sl::option_card(ui,card,balanced,"Balanced","30 fps, all media"){self.prefs.animation_fps=30;self.prefs.animations=true;self.prefs.images=true;self.prefs.idle_seconds=5;}
                    if sl::option_card(ui,card,saver,"Save memory","15 fps, media off"){self.prefs.animation_fps=15;self.prefs.animations=false;self.prefs.images=false;self.prefs.members=false;self.prefs.idle_seconds=5;}
                });
                sl::divider(ui);
                sl::label(ui,"Animation smoothness");sl::slider(ui,&mut self.prefs.animation_fps,5..=60,|v|format!("{v:.0} fps"));ui.add_space(10.);
                toggle_pref(ui,&mut self.prefs.animations,"Animate Visible Images","Play animated images that are on screen.");
                toggle_pref(ui,&mut self.prefs.images,"Show Images and GIFs","Display media inline in chat.");
                ui.horizontal(|ui|{ui.label(RichText::new(format!("{:.0} MB",self.memory.working_mb)).size(20.).strong().color(TEXT));ui.label(RichText::new("physical memory in use").color(MUTED));ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if ui.button("Clear image cache").clicked(){self.images.clear();}});});
                ui.add_space(8.);
                ui.collapsing("Advanced diagnostics",|ui|{
                    ui.label(format!("Private commit: {:.0} MiB",self.memory.private_mb));ui.label(format!("Images: {} / 128 · {:.1} / 48 MiB",self.images.len(),self.images.bytes()as f64/1048576.));
                    ui.label(format!("{} animated · {} sampled · {} static fallbacks",self.images.animated(),self.images.sampled(),self.images.fallbacks()));
                    sl::label(ui,"Idle refresh interval");sl::slider(ui,&mut self.prefs.idle_seconds,1..=10,|v|format!("{v:.0} s"));
                });
                sl::help(ui,"Recent conversations stay in a small memory cache. First visits still depend on your connection. Calls and screen sharing use additional memory.");
            }
            _=>{}
        }
    }
    /// Offline preview samples for the account list pages.
    pub(super) fn preview_account_data(&mut self){
        self.features.insert("connections".into(),json!([{"type":"spotify","id":"a","name":"lionsden_music","verified":true,"visibility":1,"show_activity":true},{"type":"steam","id":"b","name":"Evilmanfal","verified":true,"visibility":1,"show_activity":true},{"type":"github","id":"c","name":"Evilmanfal","verified":true,"visibility":0}]));
        self.features.insert("devices".into(),json!({"user_sessions":[{"id_hash":"1","current":true,"client_info":{"os":"Windows","platform":"Discord Client","location":"Toronto, Ontario, Canada"}},{"id_hash":"2","approx_last_used_time":"2026-11-03T21:14:00Z","client_info":{"os":"iOS","platform":"Discord iOS","location":"Toronto, Ontario, Canada"}},{"id_hash":"3","approx_last_used_time":"2026-10-28T09:02:00Z","client_info":{"os":"Windows","platform":"Chrome","location":"Mississauga, Ontario, Canada"}}]}));
        self.features.insert("authorized-apps".into(),json!([{"id":"t1","scopes":["identify","guilds","guilds.join"],"application":{"id":"1","name":"Carl-bot","description":"Reaction roles, automod, logging and custom commands for your server.","bot_public":true}},{"id":"t2","scopes":["identify","email"],"application":{"id":"2","name":"Medal","description":"Clip and share your best gaming moments."}}]));
    }
    /// Connections, Devices, Authorized Apps, Gifts, Boosts and Subscriptions, laid out like Discord's pages.
    fn account_page(&mut self,ui:&mut egui::Ui,page:&str){
        let key=match page{"Connections"=>"connections","Devices"=>"devices","Authorized Apps"=>"authorized-apps","Gift Inventory"=>"inventory","Server Boost"=>"boosts",_=>"subscriptions"};
        let data=self.features.get(key).cloned();
        let items:Vec<Value>=data.as_ref().and_then(|d|d.as_array().or_else(||d["user_sessions"].as_array()).cloned()).unwrap_or_default().into_iter().take(100).collect();
        let green=Color32::from_rgb(35,165,90);let red=Color32::from_rgb(242,95,100);let grey=Color32::from_gray(170);let accent=sl::accent(ui.ctx());
        if page=="Connections"{
            egui::Frame::NONE.fill(Color32::from_gray(30)).stroke(Stroke::new(1_f32,Color32::from_gray(40))).corner_radius(12).inner_margin(16).show(ui,|ui|{ui.set_width(ui.available_width());
                ui.label(RichText::new("Add accounts to your profile").size(15.).strong().color(TEXT));
                ui.label(RichText::new("This information will not be shared outside of Discord without your permission.").size(12.5).color(MUTED));ui.add_space(8.);
                ui.horizontal_wrapped(|ui|{ui.spacing_mut().item_spacing=Vec2::new(8.,8.);for service in ["spotify","steam","github","youtube","twitch","xbox","playstation","battlenet","riotgames","reddit","bluesky","crunchyroll"]{let (label,color)=service_style(service);let rect=sl::letter_icon(ui,38.,&label.chars().next().unwrap_or('?').to_string(),color);let response=ui.interact(rect,ui.id().with(service),egui::Sense::click()).on_hover_text(format!("Connect {label} in Discord"));if response.hovered(){ui.painter().rect_stroke(rect,10,Stroke::new(2_f32,Color32::WHITE),egui::StrokeKind::Outside);}if response.clicked(){ui.ctx().open_url(egui::OpenUrl::new_tab("https://discord.com/channels/@me"));}}});
            });
            ui.add_space(16.);
        }
        if page=="Devices"{sl::help(ui,"Here are all the devices currently signed in with your Discord account. Log out of any you don't recognize in Discord.");ui.add_space(10.);}
        if page=="Authorized Apps"{sl::help(ui,"Apps you have signed in to with Discord. Deauthorize any you no longer use.");ui.add_space(10.);}
        self.feature_status(ui,key);
        if data.is_some()&&items.is_empty(){
            let (glyph,title,hint)=match page{"Connections"=>("🔗","No connected accounts yet","Pick a service above to connect it in Discord."),"Devices"=>("💻","No other devices","Only this device is signed in."),"Authorized Apps"=>("🧩","No authorized apps","Apps you sign in to with Discord will appear here."),"Gift Inventory"=>("🎁","No gifts yet","Gifts you receive or buy will appear here."),"Server Boost"=>("🚀","No boosts","Boost a server to unlock perks for its members."),_=>("⭐","No subscriptions","Nitro and server subscriptions will appear here.")};
            sl::empty_state(ui,glyph,title,hint);
        }
        match page{
            "Connections"=>for(index,item)in items.iter().enumerate(){ui.push_id(index,|ui|{
                let service=item["type"].as_str().unwrap_or("").to_owned();let id=item["id"].as_str().unwrap_or("").to_owned();let (label,color)=service_style(&service);
                egui::Frame::NONE.fill(Color32::from_gray(30)).stroke(Stroke::new(1_f32,Color32::from_gray(40))).corner_radius(12).inner_margin(egui::Margin{left:16,right:16,top:14,bottom:4}).show(ui,|ui|{ui.set_width(ui.available_width());
                    ui.horizontal(|ui|{ui.spacing_mut().item_spacing.x=12.;sl::letter_icon(ui,40.,&label.chars().next().unwrap_or('?').to_string(),color);
                        ui.vertical(|ui|{ui.spacing_mut().item_spacing.y=2.;ui.horizontal(|ui|{ui.label(RichText::new(item["name"].as_str().unwrap_or("Connected account")).size(15.).strong().color(TEXT));if item["verified"].as_bool()==Some(true){sl::badge(ui,"✓ Verified",green);}if item["revoked"].as_bool()==Some(true){sl::badge(ui,"Revoked",red);}});ui.label(RichText::new(&label).size(12.5).color(MUTED));});
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if ui.add(egui::Button::new(RichText::new("✕").size(14.).color(MUTED)).frame(false)).on_hover_text("Remove connection").clicked()&&!id.is_empty(){self.confirm=Some((format!("Remove your {label} connection?"),Command::Request{key:"connections-write".into(),method:Method::DELETE,route:format!("/users/@me/connections/{service}/{id}"),body:None}));}});
                    });
                    ui.add_space(10.);let (line,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),1.),egui::Sense::hover());ui.painter().rect_filled(line,0,Color32::from_gray(44));ui.add_space(10.);
                    let mut changes=json!({});
                    let mut visible=item["visibility"].as_u64()==Some(1);if sl::toggle_row(ui,"Display on profile","",&mut visible){changes["visibility"]=json!(u8::from(visible));}
                    if item.get("show_activity").is_some(){let mut activity=item["show_activity"].as_bool()==Some(true);if sl::toggle_row(ui,&format!("Display {label} as your status"),"",&mut activity){changes["show_activity"]=json!(activity);}}
                    if changes.as_object().is_some_and(|o|!o.is_empty())&&!id.is_empty(){
                        if let Some(list)=self.features.get_mut(key).and_then(Value::as_array_mut){if let Some(entry)=list.get_mut(index){for(k,v)in changes.as_object().unwrap(){entry[k]=v.clone();}}}
                        self.mutate("connections-write",Method::PATCH,format!("/users/@me/connections/{service}/{id}"),Some(changes));
                    }
                });ui.add_space(10.);
            });},
            "Devices"=>{
                let (current,others):(Vec<_>,Vec<_>)=items.iter().partition(|i|i["current"].as_bool()==Some(true));
                for(title,group)in[("Current Device",current),("Other Devices",others)]{if group.is_empty(){continue;}sl::label(ui,title);ui.add_space(2.);
                    for(index,item)in group.iter().enumerate(){ui.push_id((title,index),|ui|{
                        let info=&item["client_info"];let os=info["os"].as_str().unwrap_or("Unknown");let platform=info["platform"].as_str().unwrap_or("");
                        let glyph=match os.to_lowercase().as_str(){"ios"|"android"=>"📱",_ if !platform.is_empty()&&!platform.to_lowercase().contains("discord")=>"🌐",_=>"💻"};
                        sl::item_card(ui,|ui|{let (rect,_)=ui.allocate_exact_size(Vec2::splat(40.),egui::Sense::hover());ui.painter().circle_filled(rect.center(),20.,Color32::from_gray(46));ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,glyph,egui::FontId::proportional(18.),TEXT);},
                            |ui|{ui.label(RichText::new(if platform.is_empty(){os.to_uppercase()}else{format!("{} · {}",os.to_uppercase(),platform.to_uppercase())}).size(13.5).strong().color(TEXT));
                                let mut line=info["location"].as_str().unwrap_or("Unknown location").to_owned();if item["current"].as_bool()!=Some(true){if let Some(date)=item["approx_last_used_time"].as_str(){line+=&format!("  •  {}",nice_date(date));}}ui.label(RichText::new(line).size(12.5).color(MUTED));},
                            |ui|{if item["current"].as_bool()==Some(true){sl::badge(ui,"This device",accent);}});
                    });}
                    ui.add_space(10.);
                }
            }
            "Authorized Apps"=>for(index,item)in items.iter().enumerate(){ui.push_id(index,|ui|{
                let app=&item["application"];let name=app["name"].as_str().unwrap_or("Connected app").to_owned();let token=item["id"].as_str().unwrap_or("").to_owned();
                let icon_url=app["id"].as_str().zip(app["icon"].as_str()).map(|(id,icon)|format!("https://cdn.discordapp.com/app-icons/{id}/{icon}.png?size=64"));
                egui::Frame::NONE.fill(Color32::from_gray(30)).stroke(Stroke::new(1_f32,Color32::from_gray(40))).corner_radius(12).inner_margin(16).show(ui,|ui|{ui.set_width(ui.available_width());
                    ui.horizontal(|ui|{ui.spacing_mut().item_spacing.x=12.;let rect=sl::letter_icon(ui,44.,&name.chars().next().unwrap_or('?').to_uppercase().to_string(),Color32::from_rgb(88,101,242));if icon_url.is_some(){self.paint_image(ui,rect,icon_url.clone(),12);}
                        ui.vertical(|ui|{ui.add_space(4.);ui.horizontal(|ui|{ui.label(RichText::new(&name).size(16.).strong().color(TEXT));if app["bot_public"].as_bool()==Some(true){sl::badge(ui,"APP",accent);}});});
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if ui.add(egui::Button::new(RichText::new("Deauthorize").color(red)).fill(Color32::TRANSPARENT).stroke(Stroke::new(1_f32,red)).corner_radius(8)).clicked()&&!token.is_empty(){self.confirm=Some((format!("Deauthorize {name}? It will lose access to your account."),Command::Request{key:"authorized-apps-write".into(),method:Method::DELETE,route:format!("/oauth2/tokens/{token}"),body:None}));}});
                    });
                    if let Some(description)=app["description"].as_str().filter(|d|!d.is_empty()){ui.add_space(12.);sl::label(ui,"About this app");let short:String=description.chars().take(220).collect();ui.label(RichText::new(if description.chars().count()>220{short+"…"}else{short}).size(13.).color(Color32::from_gray(210)));}
                    if let Some(scopes)=item["scopes"].as_array().filter(|s|!s.is_empty()){ui.add_space(12.);sl::label(ui,"Permissions");
                        for scope in scopes.iter().filter_map(Value::as_str){ui.horizontal(|ui|{let (rect,_)=ui.allocate_exact_size(Vec2::splat(16.),egui::Sense::hover());ui.painter().circle_filled(rect.center(),8.,green);let c=rect.center();let s=Stroke::new(1.6_f32,Color32::WHITE);ui.painter().line_segment([c+Vec2::new(-3.5,0.),c+Vec2::new(-1.,2.5)],s);ui.painter().line_segment([c+Vec2::new(-1.,2.5),c+Vec2::new(3.5,-2.5)],s);ui.label(RichText::new(scope_label(scope)).size(13.).color(Color32::from_gray(210)));});}
                    }
                });ui.add_space(10.);
            });},
            _=>for(index,item)in items.iter().enumerate(){ui.push_id(index,|ui|{
                let title=item["subscription_plan"]["name"].as_str().or_else(||item["guild"]["name"].as_str()).or_else(||item["sku"]["name"].as_str()).or_else(||item["name"].as_str()).unwrap_or(match page{"Server Boost"=>"Server boost","Gift Inventory"=>"Gift",_=>"Discord subscription"});
                let glyph=match page{"Server Boost"=>"🚀","Gift Inventory"=>"🎁",_=>"⭐"};
                sl::item_card(ui,|ui|{sl::letter_icon(ui,40.,glyph,Color32::from_rgb(120,72,180));},
                    |ui|{ui.label(RichText::new(title).size(15.).strong().color(TEXT));for(field,label)in[("current_period_end","Renews"),("expires_at","Expires"),("cooldown_ends_at","Available again"),("created_at","Added")]{if let Some(date)=item[field].as_str(){ui.label(RichText::new(format!("{label} {}",nice_date(date))).size(12.5).color(MUTED));break;}}},
                    |ui|{if item["cancel_at_period_end"].as_bool()==Some(true)||item["canceled"].as_bool()==Some(true){sl::badge(ui,"Cancels at period end",red);}else if item["status"].as_u64()==Some(1)||item["active"].as_bool()==Some(true){sl::badge(ui,"Active",green);}if item["available"].as_bool()==Some(false){sl::badge(ui,"Unavailable",grey);}});
            });},
        }
        for write in ["connections-write","authorized-apps-write"]{self.feature_status(ui,write);}
        if self.prefs.developer&&!items.is_empty(){ui.collapsing("Technical details",|ui|{for item in &items{for key in ["id","type","status"]{if let Some(value)=item.get(key){ui.label(format!("{key}: {value}"));}}}});}
        sl::divider(ui);
        ui.horizontal(|ui|{if ui.button("⟳  Refresh").clicked(){self.load_settings_section();}ui.add_space(6.);link(ui,"Manage in Discord","https://discord.com/channels/@me");});
    }
    fn theme_swatches(&mut self,ui:&mut egui::Ui){
        ui.horizontal_wrapped(|ui|{ui.spacing_mut().item_spacing=Vec2::new(14.,14.);
            for theme in preferences::Theme::presets(){let c=|s:&str|preferences::color(s).unwrap_or(Color32::GRAY);if sl::theme_swatch(ui,self.prefs.theme.name==theme.name,&theme.name,c(&theme.background),c(&theme.surface),c(&theme.accent),c(&theme.text)){self.prefs.theme=theme;}}
        });
    }
    pub(super) fn plugins_settings(&mut self,ui:&mut egui::Ui){
        sl::help(ui,"Native implementations of popular plugins. Vencord JavaScript plugins need Discord's web runtime and cannot be loaded here.");ui.add_space(12.);
        sl::section(ui,"Built-in Plugins");ui.add_space(4.);
        for (setting,label) in [(&mut self.prefs.spotify,"Spotify Controls · Windows media session playback"),(&mut self.prefs.volume_booster,"VolumeBooster · user and stream playback up to 1000%"),(&mut self.prefs.silent_typing,"Silent Typing · suppress outgoing typing indicators"),(&mut self.prefs.read_all,"Read All · acknowledge all known server/DM read states"),(&mut self.prefs.message_logger,"Message Logger · retain 200 received deletions / previous edits in RAM"),(&mut self.prefs.click_actions,"MessageClickActions · double-click reply, Ctrl+click copy, Alt+click edit"),(&mut self.prefs.activity_toggle,"GameActivityToggle · button beside mute"),(&mut self.prefs.role_colors,"Role Colors · color names by server role"),(&mut self.prefs.character_count,"Character Count · show remaining characters while typing")]{
            let (title,description)=label.split_once(" · ").unwrap_or((label,""));let mut description=description.to_owned();if let Some(first)=description.get(..1){description=first.to_uppercase()+&description[1..];}sl::toggle_row(ui,title,&description,setting);
        }
        ui.add_space(6.);sl::section(ui,"SpotifyCrack: No Auto-pause");sl::help(ui,"Eclipse never automatically pauses Spotify when you speak in a call. Premium spoofing and Free Listen Along are not implemented.");
        sl::divider(ui);sl::section(ui,"Local Display Plugins");ui.add_space(4.);
        let mut remove=None;let width=ui.available_width();
        for(index,plugin)in self.prefs.plugins.iter_mut().enumerate(){ui.push_id(index,|ui|{ui.horizontal(|ui|{
            ui.vertical(|ui|{ui.set_width((width-150.).max(120.));ui.label(RichText::new(&plugin.name).size(14.5).color(TEXT));if !plugin.description.is_empty(){ui.label(RichText::new(&plugin.description).size(12.5).color(MUTED));}});
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{sl::switch(ui,&mut plugin.enabled);if ui.small_button("Remove").clicked(){remove=Some(index);}});
        });sl::divider(ui);});}
        if let Some(i)=remove{self.prefs.plugins.remove(i);}
        if self.prefs.plugins.is_empty(){sl::help(ui,"No display plugins installed.");ui.add_space(6.);}
        if ui.button("Import display plugin (.json)…").clicked(){if let Some(path)=rfd::FileDialog::new().add_filter("Eclipse display plugin",&["json"]).pick_file(){let result=read_bounded(&path,64*1024).and_then(|s|preferences::Plugin::parse(&s));match result{Ok(plugin)if self.prefs.plugins.len()<32=>self.prefs.plugins.push(plugin),Ok(_)=>{self.error=Some("Maximum 32 display plugins.".into());},Err(error)=>self.error=Some(error)}}}
        sl::help(ui,"Display plugins only replace text locally. They cannot access tokens, execute JavaScript or change outgoing messages.");
    }
    pub(super) fn themes_settings(&mut self,ui:&mut egui::Ui){
        sl::section(ui,"Presets");ui.add_space(4.);self.theme_swatches(ui);sl::divider(ui);
        sl::section(ui,"Customize Colors");ui.add_space(4.);
        for(label,value)in[("Background",&mut self.prefs.theme.background),("Panels",&mut self.prefs.theme.surface),("Accent",&mut self.prefs.theme.accent),("Text",&mut self.prefs.theme.text)]{
            ui.horizontal(|ui|{ui.set_min_height(34.);ui.label(RichText::new(label).size(14.5).color(TEXT));ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{ui.spacing_mut().interact_size=Vec2::new(52.,28.);let mut rgb=preferences::color(value).unwrap_or(Color32::GRAY).to_array()[..3].try_into().unwrap();if ui.color_edit_button_srgb(&mut rgb).changed(){*value=format!("#{:02x}{:02x}{:02x}",rgb[0],rgb[1],rgb[2]);}ui.label(RichText::new(value.to_uppercase()).monospace().color(MUTED));});});
        }
        ui.add_space(10.);
        ui.horizontal(|ui|{
            if ui.button("Import JSON / CSS colors…").clicked(){if let Some(path)=rfd::FileDialog::new().add_filter("Theme",&["json","css"]).pick_file(){match read_bounded(&path,256*1024).and_then(|s|preferences::Theme::import(&s)){Ok(theme)=>self.prefs.theme=theme,Err(error)=>self.error=Some(error)}}}
            if ui.button("Export current theme…").clicked(){if let Some(path)=rfd::FileDialog::new().set_file_name("eclipse-theme.json").save_file(){if std::fs::write(path,serde_json::to_vec_pretty(&self.prefs.theme).unwrap()).is_err(){self.error=Some("Could not export theme.".into());}}}
        });
        sl::help(ui,"CSS import reads literal #RRGGBB color variables only. CSS layouts, @imports, scripts and Vencord themes do not run in a native renderer.");
    }
}
fn device_picker(ui:&mut egui::Ui,id:&str,selected:&mut Option<String>,devices:Option<&[(String,String)]>,width:f32){let label=selected.as_ref().and_then(|id|devices.and_then(|ds|ds.iter().find(|d|&d.0==id))).map(|d|d.1.as_str()).unwrap_or("System default");egui::ComboBox::from_id_salt(id).width(width).truncate().selected_text(label).show_ui(ui,|ui|{ui.selectable_value(selected,None,"System default");if let Some(devices)=devices{for (id,name)in devices{ui.selectable_value(selected,Some(id.clone()),name);}}});}
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
fn language_picker(ui:&mut egui::Ui,value:&mut Value,key:&str){let mut selected=value[key].as_str().unwrap_or("en-US").to_owned();let choices=[("en-US","English (United States)"),("en-GB","English (United Kingdom)"),("fr","Français"),("de","Deutsch"),("es-ES","Español"),("pt-BR","Português (Brasil)"),("ja","日本語"),("ko","한국어"),("zh-CN","简体中文"),("ru","Русский"),("uk","Українська"),("it","Italiano"),("nl","Nederlands"),("pl","Polski"),("tr","Türkçe")];sl::label(ui,"Language");egui::ComboBox::from_id_salt(key).width(ui.available_width().min(340.)).selected_text(choices.iter().find(|(code,_)|*code==selected).map(|(_,label)|*label).unwrap_or("Current account language")).show_ui(ui,|ui|{for(code,label)in choices{ui.selectable_value(&mut selected,code.into(),label);}});value[key]=json!(selected);}
fn suppression_label(p:voice_model::voice_settings::NoiseSuppression)->&'static str{use voice_model::voice_settings::NoiseSuppression::*;match p{Off=>"Off",RnNoise=>"Enhanced noise reduction",WebRtc=>"Standard noise reduction"}}
fn audit_action(n:u64)->&'static str{match n{1=>"Server updated",10=>"Channel created",11=>"Channel updated",12=>"Channel deleted",20=>"Member kicked",22=>"Member banned",23=>"Member unbanned",24=>"Member updated",25=>"Member roles updated",30=>"Role created",31=>"Role updated",32=>"Role deleted",40=>"Invite created",41=>"Invite updated",42=>"Invite deleted",60=>"Emoji created",61=>"Emoji updated",62=>"Emoji deleted",72=>"Message deleted",73=>"Messages deleted",74=>"Message pinned",75=>"Message unpinned",_=>"Server activity"}}
/// Display name and brand colour for a Discord connection type.
fn service_style(service:&str)->(String,Color32){
    let rgb=Color32::from_rgb;
    let (name,color)=match service{"spotify"=>("Spotify",rgb(30,185,84)),"steam"=>("Steam",rgb(42,71,94)),"github"=>("GitHub",rgb(36,41,47)),"youtube"=>("YouTube",rgb(230,33,23)),"twitch"=>("Twitch",rgb(145,70,255)),"xbox"=>("Xbox",rgb(16,124,16)),"playstation"=>("PlayStation",rgb(0,55,145)),"reddit"=>("Reddit",rgb(255,69,0)),"twitter"=>("X",rgb(20,20,20)),"battlenet"=>("Battle.net",rgb(20,142,255)),"epicgames"=>("Epic Games",rgb(49,49,49)),"riotgames"=>("Riot Games",rgb(209,54,57)),"leagueoflegends"=>("League of Legends",rgb(194,143,44)),"tiktok"=>("TikTok",rgb(255,0,80)),"instagram"=>("Instagram",rgb(225,48,108)),"facebook"=>("Facebook",rgb(24,119,242)),"domain"=>("Domain",rgb(88,101,242)),"paypal"=>("PayPal",rgb(0,48,135)),"ebay"=>("eBay",rgb(229,50,56)),"crunchyroll"=>("Crunchyroll",rgb(244,117,33)),"roblox"=>("Roblox",rgb(226,35,26)),"bluesky"=>("Bluesky",rgb(17,133,254)),"bungie"=>("Bungie.net",rgb(11,122,209)),"mastodon"=>("Mastodon",rgb(99,100,255)),_=>("",rgb(78,80,88))};
    (if name.is_empty(){friendly_name(service)}else{name.into()},color)
}
/// "2026-11-03T..." becomes "Nov 3, 2026".
fn nice_date(date:&str)->String{
    let parts:Vec<u32>=date.get(..10).unwrap_or("").split('-').filter_map(|p|p.parse().ok()).collect();
    match parts[..]{[y,m@1..=12,d]=>format!("{} {d}, {y}",["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"][m as usize-1]),_=>date.get(..10).unwrap_or(date).to_owned()}
}
/// OAuth scope as the permission Discord describes.
fn scope_label(scope:&str)->String{match scope{"identify"=>"Access your username, avatar and banner".into(),"email"=>"Access your email address".into(),"guilds"=>"Know what servers you're in".into(),"guilds.join"=>"Join servers for you".into(),"guilds.members.read"=>"Read your server member info".into(),"connections"=>"Access your third-party connections".into(),"bot"=>"Add a bot to a server".into(),"applications.commands"=>"Create commands".into(),"rpc"=>"Talk to your Discord client".into(),"messages.read"=>"Read your messages".into(),"activities.write"=>"Update your activity".into(),"role_connections.write"=>"Update your linked roles".into(),other=>friendly_name(&other.replace('.',"_"))}}
#[cfg(test)]mod account_tests{use super::*;
    #[test]fn dates_and_services_read_naturally(){assert_eq!(nice_date("2026-11-03T14:01:00Z"),"Nov 3, 2026");assert_eq!(nice_date("bad"),"bad");assert_eq!(service_style("github").0,"GitHub");assert_eq!(service_style("new_service").0,"New Service");assert_eq!(scope_label("guilds.join"),"Join servers for you");}
}
