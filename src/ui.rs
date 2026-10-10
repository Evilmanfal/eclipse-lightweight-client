use crate::{
    assets::{self, Images},
    backend::{self, Backend, Command, Event},
    folders::{Layout, RailEntry},
    model::*,
    Memory,
};
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroize;

const BG: Color32 = Color32::from_gray(11);
const RAIL: Color32 = Color32::from_gray(14);
const SIDE: Color32 = Color32::from_gray(23);
const CARD: Color32 = Color32::from_gray(36);
const ACCENT: Color32 = Color32::from_gray(184);
const MUTED: Color32 = Color32::from_gray(170);
const TEXT: Color32 = Color32::from_gray(237);
const LOG_RED: Color32 = Color32::from_rgb(242, 124, 133);
const BORDER: Color32 = Color32::from_gray(46);
const DM_PAGE_SIZE: usize = 25;
#[path="panels.rs"] mod panels;
#[derive(Clone,Copy,PartialEq)] enum Home { Chat, Friends, Nitro, Shop, Quests }
use crate::timeline::Logged;

pub struct Eclipse {
    backend: Option<Backend>,
    preview: bool,
    preview_gesture: Option<&'static str>,
    message_clock: crate::message_time::Clock,
    connecting: bool,
    user: Option<User>,
    guilds: Vec<Guild>,
    dms: Vec<Channel>,
    dm_visible: usize,
    channels: Vec<Channel>,
    guild: Option<String>,
    channel: Option<Channel>,
    messages: VecDeque<Message>,
    navigation: crate::navigation::Cache,
    login_ui: panels::LoginUi,
    /// Token from a sign-in in progress; saved to Credential Manager once Discord accepts it.
    pending_save: Option<zeroize::Zeroizing<String>>,
    /// This session was restored from the saved sign-in at launch.
    auto_login: bool,
    error: Option<String>,
    status: String,
    drafts: HashMap<String, String>,
    files: HashMap<String, PathBuf>,
    /// Paste keys were held last frame, so one Ctrl+V attaches once.
    paste_down: bool,
    /// Everyone in voice channels and DM calls, including people using other Discord clients.
    voice: crate::voice_roster::VoiceRoster,
    /// Users already requested from the gateway so voice rows can show their names.
    voice_lookups: HashSet<String>,
    pending: Option<(String, String)>,
    nonces: HashMap<String, (String, Option<PathBuf>, String,Option<String>)>,
    search: String,
    channel_filter: String,
    settings: bool,
    /// The message being edited in place, and its edited text.
    edit: Option<(String, String)>,
    focus_edit: bool,
    delete: Option<String>,
    pins: Option<Vec<Message>>,
    pins_anchor: Option<egui::Rect>,
    pins_just_opened: bool,
    /// Message to scroll to, with a few frames of retries while layout settles.
    jump_to: Option<(String,u8)>,
    highlight: Option<(String,f64)>,
    /// The loaded history is a window around an older message rather than the latest messages.
    detached: bool,
    full_profile: Option<User>,
    full_profile_guild: Option<String>,
    full_profile_tab: u8,
    /// Account menu above the bottom-left bar: anchor, opened this frame, expanded row (status / switch).
    account_anchor: Option<egui::Rect>,
    account_just_opened: bool,
    account_expanded: Option<u8>,
    dm_search: String,
    dm_modal: bool,
    dm_anchor: Option<egui::Rect>,
    dm_just_opened: bool,
    loaded: bool,
    has_older: bool,
    unread: HashMap<String, usize>,
    compact: bool,
    show_members: bool,
    memory: Memory,
    last_sample: Instant,
    started: Instant,
    smoke: Option<String>,
    screenshot_requested: bool,
    screenshot_saved: bool,
    images: Images,
    layout: Layout,
    open_folders: HashSet<String>,
    folder_status: String,
    font_scan_needed: bool,
    extra_emoji_font: bool,
    subscribed_guild: Option<String>,
    incoming_call: Option<Channel>,
    presences: crate::presence::Presences,
    picker: crate::media_picker::Picker,
    calls: crate::calls::Calls,
    prefs:crate::preferences::Preferences,
    applied_prefs:crate::preferences::Preferences,
    prefs_save_at:Option<Instant>,
    home:Home,
    settings_page:String,
    settings_search:String,
    server_settings:bool,
    server_page:String,
    server:crate::community::Server,
    features:HashMap<String,serde_json::Value>,
    feature_errors:HashMap<String,String>,
    feature_pending:HashSet<String>,
    account_edit:serde_json::Value,
    settings_edit:serde_json::Value,
    server_edit:serde_json::Value,
    role_edit:Option<crate::community::Role>,
    profile:Option<User>,
    friends:Vec<crate::community::Relationship>,
    friend_filter:String,
    friend_search:String,
    friend_add:String,
    member_search:String,
    reply:Option<Message>,
    logs:VecDeque<Logged>,
    profile_anchor: Option<egui::Rect>,
    profile_guild: Option<String>,
    profile_just_opened: bool,
    voice_revealed: Option<String>,
    shop_filter: String,
    quest_filter: String,
    spotify:Option<crate::spotify::Spotify>,
    game_activity:bool,
    read_latest:HashMap<String,String>,
    confirm:Option<(String,Command)>,
    hotkey_record:Option<crate::hotkey::Recorder>,
    audio_devices:Option<discord_voice::audio::DeviceList>,
    last_typing:Option<Instant>,
    composer_ime: bool,
    /// The @ suggestion list: open last frame, highlighted row, picked names → user ids, last query sent.
    mention_open: bool,
    mention_pick: usize,
    mention_ids: HashMap<String, String>,
    mention_query: String,
    /// Custom emojis of every server you are in, by server id: (server name, emojis).
    server_emojis: HashMap<String, (String, Vec<crate::media_picker::CustomEmoji>)>,
    /// When each stream preview picture was last asked for, by stream key.
    stream_preview_fetched: HashMap<String, Instant>,
    /// Put the cursor in the message box next frame (after Reply).
    focus_message_box: bool,
    updater: crate::updater::Updater,
}
impl Eclipse {
    pub fn new(cc: &eframe::CreationContext<'_>, preview: bool, smoke: Option<String>) -> Self {
        let restore=!preview&&smoke.is_none();
        let mut app=Self::with_context(&cc.egui_ctx,preview,smoke);
        // Stay signed in: resume the session saved in Windows Credential Manager.
        if restore{if let Some(token)=crate::login::saved::load(){app.begin_session(token,&cc.egui_ctx);app.auto_login=true;}}
        // Look for a newer release on GitHub; a prompt appears if there is one.
        if restore{app.updater.check(&cc.egui_ctx);}
        app
    }
    fn with_context(ctx:&egui::Context,preview:bool,smoke:Option<String>)->Self {
        crate::widgets::fonts(&ctx);
        ctx.set_theme(egui::ThemePreference::Dark);
        let mut style = (*ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.override_text_color = Some(TEXT);
        style.visuals.panel_fill = BG;
        style.visuals.window_fill = CARD;
        style.visuals.extreme_bg_color = SIDE;
        style.visuals.faint_bg_color = CARD;
        style.visuals.selection.bg_fill = Color32::from_gray(65);
        style.visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
        style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
        style.visuals.widgets.inactive.weak_bg_fill = CARD;
        style.visuals.widgets.hovered.weak_bg_fill = Color32::from_gray(52);
        style.visuals.widgets.active.weak_bg_fill = Color32::from_gray(64);
        style.spacing.item_spacing = Vec2::new(10.0, 8.0);
        style.spacing.button_padding = Vec2::new(12.0, 8.0);
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(24.0));
        ctx.set_style(style);
        let prefs=crate::preferences::Preferences::load();
        prefs.apply(&ctx);
        let mut app = Self {
            backend: None,
            preview,
            preview_gesture: None,
            message_clock: Default::default(),
            connecting: false,
            user: None,
            guilds: vec![],
            dms: vec![],
            dm_visible: DM_PAGE_SIZE,
            channels: vec![],
            guild: None,
            channel: None,
            messages: VecDeque::new(),
            navigation: Default::default(),
            login_ui: Default::default(),
            pending_save: None,
            auto_login: false,
            error: None,
            status: "Offline".into(),
            drafts: HashMap::new(),
            files: HashMap::new(),
            paste_down: false,
            voice: Default::default(),
            voice_lookups: HashSet::new(),
            pending: None,
            nonces: HashMap::new(),
            search: String::new(),
            channel_filter: String::new(),
            settings: false,
            edit: None,
            focus_edit: false,
            delete: None,
            pins: None,
            pins_anchor: None,
            pins_just_opened: false,
            jump_to: None,
            highlight: None,
            detached: false,
            full_profile: None,
            full_profile_guild: None,
            full_profile_tab: 0,
            account_anchor: None,
            account_just_opened: false,
            account_expanded: None,
            dm_search: String::new(),
            dm_modal: false,
            dm_anchor:None,dm_just_opened:false,
            loaded: false,
            has_older: true,
            unread: HashMap::new(),
            compact: prefs.compact,
            show_members: prefs.members,
            memory: crate::memory(),
            last_sample: Instant::now(),
            started: Instant::now(),
            smoke,
            screenshot_requested: false,
            screenshot_saved: false,
            images: Images::new(&ctx),
            layout: Layout::default(),
            open_folders: HashSet::new(),
            folder_status: "Waiting for server folders…".into(),
            font_scan_needed: true,
            extra_emoji_font: false,
            subscribed_guild: None,
            incoming_call: None,
            presences: Default::default(),
            picker: Default::default(),
            calls: crate::calls::Calls::new(ctx.clone()),
            applied_prefs:prefs.clone(),prefs,prefs_save_at:None,home:Home::Chat,
            settings_page:"Account & Profile".into(),settings_search:String::new(),server_settings:false,server_page:"Overview".into(),server:Default::default(),features:HashMap::new(),feature_errors:HashMap::new(),feature_pending:HashSet::new(),account_edit:serde_json::Value::Null,settings_edit:serde_json::Value::Null,server_edit:serde_json::Value::Null,role_edit:None,profile:None,friends:vec![],friend_filter:"Online".into(),friend_search:String::new(),friend_add:String::new(),member_search:String::new(),reply:None,logs:VecDeque::new(),profile_anchor:None,profile_guild:None,profile_just_opened:false,voice_revealed:None,shop_filter:"All".into(),quest_filter:"Discover".into(),spotify:None,game_activity:true,read_latest:HashMap::new(),confirm:None,hotkey_record:None,audio_devices:None,last_typing:None,composer_ime:false,mention_open:false,mention_pick:0,mention_ids:HashMap::new(),mention_query:String::new(),server_emojis:HashMap::new(),stream_preview_fetched:HashMap::new(),focus_message_box:false,updater:Default::default(),
        };
        app.calls.configure(&app.prefs);
        app.images.playback_options(app.prefs.animations&&!app.prefs.reduced_motion,app.prefs.animation_fps);
        if preview {
            app.load_demo();
        }
        app
    }
    fn surface_color(&self)->Color32{crate::preferences::color(&self.prefs.theme.surface).unwrap_or(SIDE)}
    /// Compact mode joins the panels into one square-edged surface; separator lines divide them.
    fn surface(&self)->egui::Frame{if self.compact{egui::Frame::NONE.fill(self.surface_color())}else{surface().fill(self.surface_color())}}
    /// The outer frame of a docked panel: a background gap around a rounded card, or no gap in compact mode.
    fn panel_frame(&self,margin:egui::Margin)->egui::Frame{if self.compact{egui::Frame::NONE.fill(self.surface_color())}else{egui::Frame::NONE.fill(preferences_bg(&self.prefs)).inner_margin(margin)}}
    /// Inner padding of a panel card, reduced in compact mode.
    fn pad(&self,normal:i8)->i8{if self.compact{(normal*3/5).max(2)}else{normal}}
    fn accent(&self)->Color32{crate::preferences::color(&self.prefs.theme.accent).unwrap_or(ACCENT)}
    fn load_demo(&mut self) {
        let (user, guilds, channels, messages) = demo();
        self.preview = true;
        for (index, message) in messages.iter().enumerate() {
            self.presences.set(
                message.author.id.clone(),
                String::new(),
                [
                    crate::presence::Status::Online,
                    crate::presence::Status::Idle,
                    crate::presence::Status::Dnd,
                    crate::presence::Status::Offline,
                ][index % 4],
            );
        }
        self.user = Some(user);
        self.guild = Some(guilds[0].id.clone());
        self.guilds = guilds;
        self.layout = Layout::demo();
        self.open_folders.insert("creative".into());
        self.folder_status = "Offline sample folders".into();
        self.channel = channels.iter().find(|c| c.is_text()).cloned();
        self.channels = channels;
        self.messages = messages;
        let mut people = HashMap::new();
        for message in &self.messages {
            if message.author.id != "local-me" {
                people.insert(message.author.id.clone(), message.author.clone());
            }
        }
        self.dms = people
            .into_values()
            .map(|user| Channel {
                id: format!("dm-{}", user.id),
                kind: 1,
                recipients: vec![user],
                ..Default::default()
            })
            .collect();
        self.dms.sort_by_key(Channel::label);
        self.loaded = true;
        self.has_older = false;
        self.status = "Offline preview".into();
        self.demo_community();
    }
    pub fn stress_preview(&mut self, ctx: &egui::Context) {
        self.load_demo();
        self.images.stress_fill(ctx);
        for i in 0..HISTORY_LIMIT {
            merge_message(&mut self.messages,Message{
                id:format!("stress-{i}"),channel_id:"local-0".into(),author:self.user.clone().unwrap(),
                content:format!("Memory workload message {i}. {}", "Native text history stays bounded while the interface remains responsive. ".repeat(25)),
                timestamp:"12:00".into(),..Default::default()
            });
        }
    }
    pub fn preview_options(&mut self, collapsed: bool, dms: bool) {
        if !self.preview {
            return;
        }
        if collapsed {
            self.open_folders.clear();
        }
        if dms {
            self.guild = None;
            self.channels = self.dms.clone();
            self.channel = None;
            self.messages.clear();
        }
    }
    pub fn preview_section(&mut self,section:&str){
        if !self.preview{return;}
        match section {
            "timestamps"=>{self.messages.drain(..self.messages.len().saturating_sub(3));for(message,days)in self.messages.iter_mut().rev().zip(0..3){message.timestamp=crate::message_time::preview_timestamp(days);}},
            "emoji"|"gifs"=>{if let Some(channel)=&self.channel{self.picker.open(if section=="emoji"{crate::media_picker::Mode::Emoji}else{crate::media_picker::Mode::Gif},&channel.id,self.guild.as_deref(),egui::Rect::from_min_size(egui::pos2(960.,780.),Vec2::splat(30.)));}if section=="gifs"{self.picker.set_categories([crate::media_picker::TRENDING,"hello","lol","love","happy birthday","thank you"].iter().map(|n|(n.to_string(),String::new())).collect());}},

            "link-preview"=>{if let (Some(user),Some(last))=(self.user.clone(),self.messages.back().cloned()){let link="https://stremio-addons.net/addons/magnetflix";let mut message=Message{id:"link-preview".into(),author:user,content:link.into(),..last};message.referenced_message=None;message.reactions.clear();message.attachments.clear();message.embeds=vec![crate::model::Embed{kind:"rich".into(),title:Some("Magnetflix".into()),description:Some("Addon de filmes, séries e animes dublados e legendados em Português (PT-BR)".into()),url:Some(link.into()),color:Some(0xb06cf0),provider:Some(crate::model::EmbedName{name:Some("Stremio Addons".into()),url:None}),..Default::default()}];self.messages.push_back(message);}},
            "update-prompt"=>self.updater.preview(false),
            "update-button"=>self.updater.preview(true),
            "inline-edit"=>{if let (Some(user),Some(message))=(self.user.clone(),self.messages.back_mut()){message.author=user;let message=message.clone();self.start_edit(&message);}},
            "mentions"=>{if let Some(channel)=self.channel.clone(){self.drafts.insert(channel.id.clone(),"@".into());self.focus_message_box=true;}},
            "zoom-in"=>self.prefs.zoom=1.5,
            "zoom-out"=>self.prefs.zoom=0.75,
            "compact"=>self.prefs.compact=true,
            "settings-zoom"=>{self.prefs.zoom=1.5;self.settings=true;},
            "server-zoom"=>{self.prefs.zoom=1.5;self.server_settings=true;},
            "quick-actions"=>{self.preview_gesture=Some("message");if let Some(message)=self.messages.back_mut(){if let Some(user)=&self.user{message.author=user.clone();}}},
            "server-hover"=>self.preview_gesture=Some("server"),
            "full-profile"=>{if let Some(mut user)=self.messages.front().map(|m|m.author.clone()){user.display_name_styles=Some(serde_json::json!({"font_id":14,"effect_id":2,"colors":[12034809,4437965]}));self.open_full_profile(&user);let key=crate::ui::panels::profile_key_for(&user.id,self.full_profile_guild.as_deref());self.features.insert(key,serde_json::json!({"user_profile":{"pronouns":"they / them","bio":"Building little things with good people.
A little more room to breathe.","theme_colors":[7558305,2498598]},"guild_member":{"joined_at":"2021-10-13T00:00:00Z"},"connected_accounts":[{"type":"twitch","name":"eclipse","verified":true},{"type":"xbox","name":"Eclipse"}]}));}},
            "account-menu"=>{self.account_anchor=Some(egui::Rect::from_min_size(egui::pos2(4.,814.),Vec2::new(311.,52.)));self.account_just_opened=true;self.account_expanded=Some(0);},
            "pins"=>{self.pins_anchor=Some(egui::Rect::from_min_size(egui::pos2(1144.,17.),Vec2::splat(28.)));self.pins_just_opened=true;self.pins=Some(self.messages.iter().filter(|m|m.pinned).cloned().collect());},
            "settings"=>{self.settings=true;self.settings_page="Account & Profile".into();},
            "dm-picker"=>{self.navigate_home(Home::Friends);self.open_dm_picker(egui::Rect::from_min_size(egui::pos2(250.,338.),Vec2::new(28.,24.)));self.dm_search="a".into();},
            "profile"=>{if let Some(mut user)=self.messages.front().map(|m|m.author.clone()){user.display_name_styles=Some(serde_json::json!({"font_id":14,"effect_id":2,"colors":[12034809,4437965]}));self.show_profile_at(&user,egui::Rect::from_min_size(egui::pos2(1110.,175.),Vec2::new(200.,40.)));let key=self.profile_key(&user.id);self.features.insert(key,serde_json::json!({"user_profile":{"pronouns":"they / them","bio":"Building little things with good people.\nA little more room to breathe.","theme_colors":[7558305,2498598]}}));}},
            "inline-logs"=>{if let Some(old)=self.messages.get(self.messages.len().saturating_sub(2)).cloned(){self.log_message(old.clone(),false);let index=self.messages.len().saturating_sub(2);if let Some(message)=self.messages.get_mut(index){message.content="The updated message stays right here in the conversation.".into();message.edited_timestamp=Some("edited".into());}}if let Some(deleted)=self.messages.back().cloned(){self.log_message(deleted.clone(),true);self.messages.retain(|m|m.id!=deleted.id);}},
            "voice-sidebar"=>{if let(Some(channel),Some(user))=(self.channels.iter().find(|c|c.kind==2).cloned(),self.user.clone()){self.calls.preview(channel,user,vec![]);self.calls.chat=true;}},
            "voice"|"plugins"|"themes"|"performance"=>{self.settings=true;self.settings_page=match section{"voice"=>"Voice & Video","plugins"=>"Plugins","themes"=>"Themes",_=>"Performance"}.into();},
            "server-settings"=>{self.server_settings=true;self.server_page="Overview".into();},
            "roles"=>{self.server_settings=true;self.server_page="Roles".into();},
            "friends"=>{self.navigate_home(Home::Friends);self.friend_filter="All".into();},
            "nitro"=>self.navigate_home(Home::Nitro),"shop"=>self.navigate_home(Home::Shop),"quests"=>self.navigate_home(Home::Quests),
            "call-live"|"call-watching"|"stream-pip"|"stream-expanded"|"share-picker"=>{if let(Some(channel),Some(user))=(self.channel.clone(),self.user.clone()){let mut seen=HashSet::new();let peers=self.messages.iter().map(|m|m.author.clone()).filter(|u|u.id!=user.id&&seen.insert(u.id.clone())).take(3).collect();self.calls.preview(channel,user,peers);if section!="share-picker"{self.calls.preview_live(section!="call-live");self.calls.chat=section=="stream-pip";self.calls.expanded=section=="stream-expanded";}else{self.calls.preview_picker();}}},
            "call"=>{if let(Some(channel),Some(user))=(self.channel.clone(),self.user.clone()){let mut seen=HashSet::new();let peers=self.messages.iter().map(|m|m.author.clone()).filter(|u|u.id!=user.id&&seen.insert(u.id.clone())).take(3).collect();self.calls.preview(channel,user,peers);}},
            _=>{}
        }
    }
    fn disconnect(&mut self) {
        self.backend = None;
        self.navigation = Default::default();
        self.preview = false;
        self.connecting = false;
        self.user = None;
        self.guilds.clear();
        self.calls.disconnect();
        self.presences = Default::default();
        self.picker = Default::default();
        self.subscribed_guild = None;
        self.incoming_call = None;
        self.images.clear();
        self.layout = Layout::default();
        self.open_folders.clear();
        self.folder_status = "Waiting for server folders…".into();
        self.channels.clear();
        self.dms.clear();
        self.channel = None;
        self.guild = None;
        self.messages.clear();
        self.pending = None;
        self.drafts.clear();
        self.files.clear();
        self.unread.clear();
        self.nonces.clear();
        self.status = "Offline".into();
        self.error = None;
        self.search.clear();
        self.features.clear();self.feature_errors.clear();self.feature_pending.clear();self.server=Default::default();self.voice.clear();self.voice_lookups.clear();self.friends.clear();self.profile=None;self.account_edit=serde_json::Value::Null;self.settings_edit=serde_json::Value::Null;self.server_edit=serde_json::Value::Null;self.logs.clear();self.reply=None;self.read_latest.clear();self.confirm=None;self.home=Home::Chat;
    }
    fn send_command(&mut self, command: Command) -> bool {
        match self.backend.as_ref().map(|b| b.tx.try_send(command)) {
            Some(Ok(())) => true,
            Some(Err(_)) => {
                self.error = Some(
                    "The connection is busy or closed. Try again, or disconnect and reconnect."
                        .into(),
                );
                false
            }
            None => false,
        }
    }
    fn send_gateway(&mut self, payload: serde_json::Value) {
        if let Some(backend) = &self.backend {
            if backend.gateway.try_send(payload).is_err() {
                self.error = Some("Live connection is busy; retry shortly.".into());
            }
        }
    }
    fn start_call(&mut self, channel: &Channel, video: bool) {
        if self.preview {
            self.error = Some("Connect to Discord to make a call.".into());
            return;
        }
        if let Some(user) = &self.user {
            match self.calls.join(channel, user, video) {
                Ok(payload) => {
                    self.send_gateway(payload);
                    if channel.guild_id.is_none() {
                        let (intent, epoch) = self.calls.intent();
                        self.send_command(Command::Ring(channel.id.clone(), intent, epoch));
                    }
                }
                Err(error) => self.error = Some(error.into()),
            }
        }
    }
    fn subscribe(&mut self, guild: Option<&str>, channel: Option<&str>) {
        if self.preview {
            return;
        }
        if self.subscribed_guild.as_deref() != guild {
            if let Some(old) = self.subscribed_guild.take() {
                self.send_gateway(serde_json::json!({"op":37,"d":{"subscriptions":{old:{"typing":false,"threads":false,"activities":false,"member_updates":false,"members":[],"channels":{},"thread_member_lists":[]}}}}));
            }
            self.subscribed_guild = guild.map(str::to_owned);
            if let Some(guild) = guild {
                self.send_gateway(serde_json::json!({"op":37,"d":{"subscriptions":{guild:{"typing":true,"threads":false,"activities":true,"member_updates":false,"members":[],"channels":{},"thread_member_lists":[]}}}}));
            }
        }
        if let (Some(guild), Some(channel)) = (guild, channel) {
            self.send_gateway(serde_json::json!({"op":37,"d":{"subscriptions":{guild:{"typing":true,"threads":false,"activities":true,"member_updates":false,"members":[],"channels":{channel:[[0,99]]},"thread_member_lists":[]}}}}));
        }
    }
    fn select_channel(&mut self, channel: Channel) {
        if self.calls.active() && channel.kind != 2 { self.calls.chat = true; }
        self.home=Home::Chat;
        self.profile=None;
        self.reply=None;
        if self.channel.as_ref().is_some_and(|c| c.id == channel.id) {
            return;
        }
        self.remember_conversation();
        self.navigation.select(&channel);
        self.subscribe(channel.guild_id.as_deref(), Some(&channel.id));
        self.search.clear();
        self.composer_ime=false;
        self.messages.clear();
        self.pins = None;
        self.pins_anchor = None;
        self.jump_to = None;
        self.highlight = None;
        self.detached = false;
        self.loaded = false;
        self.has_older = true;
        self.unread.remove(&channel.id);
        if self.preview {
            self.loaded = true;
            self.has_older = false;
            if channel.id == "local-0" {
                self.messages = demo().3;
            }
        } else {
            if let Some((messages,older))=self.navigation.get(&channel.id){self.messages=messages;self.has_older=older;self.loaded=true;}
            self.send_command(Command::History(channel.id.clone(), None));
        }
        self.channel = Some(channel);
    }
    fn handle_events(&mut self) {
        let events: Vec<_> = self
            .backend
            .as_ref()
            .map(|b| b.rx.try_iter().take(64).collect())
            .unwrap_or_default();
        self.font_scan_needed|=!events.is_empty();
        for event in events {
            match event {
                Event::Data(key,result)=>self.feature_result(key,result),
                Event::Account(kind,data)=>self.account_event(&kind,&data),
                Event::Presence(data) => {
                    if let (Some(user), Some(status)) =
                        (&self.user, data["user_settings"]["status"].as_str())
                    {
                        self.presences.set(
                            user.id.clone(),
                            String::new(),
                            crate::presence::Status::parse(status),
                        );
                    }
                    self.presences.ingest(&data);
                }
                Event::Signal(kind, data) => {
                    self.voice.ingest(&kind, &data);
                    if matches!(kind.as_str(), "CALL_CREATE" | "CALL_UPDATE") {
                        let ringing = data["ringing"].as_array().is_some_and(|ids| {
                            self.user
                                .as_ref()
                                .is_some_and(|u| ids.iter().any(|id| id.as_str() == Some(&u.id)))
                        });
                        if ringing {
                            self.incoming_call = self
                                .dms
                                .iter()
                                .find(|c| data["channel_id"].as_str() == Some(&c.id))
                                .cloned();
                        } else if self
                            .incoming_call
                            .as_ref()
                            .is_some_and(|c| data["channel_id"].as_str() == Some(&c.id))
                        {
                            self.incoming_call = None;
                        }
                    }
                    if kind == "CALL_DELETE"
                        && self
                            .incoming_call
                            .as_ref()
                            .is_some_and(|c| data["channel_id"].as_str() == Some(&c.id))
                    {
                        self.incoming_call = None;
                    }
                    self.calls.signal(&kind, &data);
                }
                Event::Ringing => {}
                Event::Emojis(guild, emojis) => {
                    if self.picker.guild.as_deref() == Some(&guild) {
                        self.picker.emojis = emojis;
                    }
                }
                Event::GifCategories(categories) => {
                    self.picker.set_categories(categories);
                }
                Event::Gifs(query, gifs) => {
                    if self.picker.requested == query {
                        self.picker.pending = false;
                        self.picker.gifs = gifs;
                    }
                }
                Event::Folders(layout) => {
                    self.folder_status = format!(
                        "{} account folders loaded",
                        layout.folders.iter().filter(|f| f.id.is_some()).count()
                    );
                    self.layout = layout;
                    let ids: HashSet<_> = self
                        .layout
                        .folders
                        .iter()
                        .filter_map(|f| f.id.clone())
                        .collect();
                    self.open_folders.retain(|id| ids.contains(id));
                }
                Event::FolderStatus(status) => {
                    // A successful Gateway import may have beaten the optional REST request.
                    if self.folder_status.starts_with("Waiting") {
                        self.folder_status = status;
                    }
                }
                Event::ProfilePatch(kind, data) => self.profile_patch(&kind, &data),
                Event::Connected(user, guilds, dms) => {
                    self.connecting = false;
                    if let Some(token) = self.pending_save.take() { crate::login::saved::save(&token); }
                    self.auto_login = false;
                    self.login_ui = Default::default();
                    self.user = Some(user);
                    self.guilds = guilds;

                    self.dms = dms;
                    crate::navigation::sort_dms(&mut self.dms);
                    self.dm_visible = DM_PAGE_SIZE;
                    for (channel,message) in self.dms.iter().filter_map(|c|c.last_message_id.as_ref().map(|id|(c.id.clone(),id.clone()))).collect::<Vec<_>>(){self.note_latest(channel,message);}
                    self.error = None;
                    if self.guild.is_none() {
                        self.channels = self.dms.clone();
                    }
                }
                Event::Channels(guild, mut channels) => {
                    for c in &channels{if let Some(last)=&c.last_message_id{self.note_latest(c.id.clone(),last.clone());}}
                    crate::navigation::sort_channels(&mut channels);
                    self.navigation.save_channels(&guild,&channels);
                    if self.guild.as_deref() == Some(&guild) {
                        self.channels = channels;
                        if self.channel.is_none() {
                            if let Some(channel) =
                                self.channels.iter().find(|c| c.is_text()).cloned()
                            {
                                self.select_channel(channel);
                            }
                        }
                    }
                }
                Event::History(channel, messages, older) => {
                    if !self.channel.as_ref().is_some_and(|c|c.id==channel)&&!older{let has_older=messages.len()==50;self.navigation.save(&channel,&history(messages.clone()),has_older);}
                    if let Some(guild) = self.guild.clone() {
                        let users: HashSet<_> =
                            messages.iter().map(|m| m.author.id.clone()).collect();
                        self.send_gateway(serde_json::json!({"op":8,"d":{"guild_id":guild,"user_ids":users.into_iter().take(100).collect::<Vec<_>>(),"presences":true}}));
                    }
                    if self.channel.as_ref().is_some_and(|c| c.id == channel) {
                        if older {
                            self.has_older = messages.len() == 50;
                        } else if !self.loaded {
                            self.has_older = messages.len() == 50;
                        }
                        if !older&&self.detached{self.messages.clear();self.detached=false;}
                        let mut combined: Vec<_> = self.messages.drain(..).collect();
                        for message in messages {
                            if let Some(old) = combined.iter_mut().find(|m| m.id == message.id) {
                                *old = message;
                            } else {
                                combined.push(message);
                            }
                        }
                        self.messages = history(combined);
                        self.loaded = true;
                    }
                }
                Event::Around(channel, messages, target) => {
                    if self.channel.as_ref().is_some_and(|c| c.id == channel) {
                        if messages.iter().any(|m| m.id == target) {
                            self.messages = history(messages);
                            self.has_older = true;
                            self.loaded = true;
                            self.detached = true;
                            self.search.clear();
                            self.jump_to = Some((target, 4));
                        } else {
                            self.error = Some("That message is no longer available.".into());
                        }
                    }
                }
                Event::Message(message) => self.receive_message(message, true),
                Event::Sent(message) => {
                    if let Some((channel, content)) = self.pending.take() {
                        if self.drafts.get(&channel) == Some(&content) {
                            self.drafts.remove(&channel);
                        }
                        self.files.remove(&channel);
                        self.nonces.remove(&channel);
                    }
                    self.error = None;self.reply=None;
                    self.receive_message(message, false);
                }
                Event::SendFailed(error) => {
                    self.pending = None;
                    self.error = Some(error);
                }
                Event::Deleted(channel, id) => {
                    self.navigation.invalidate(&channel);
                    if self.prefs.message_logger {if let Some(message)=self.messages.iter().find(|m|m.channel_id==channel&&m.id==id).cloned(){self.log_message(message,true);}}
                    if self.channel.as_ref().is_some_and(|c| c.id == channel) {
                        self.messages.retain(|m| m.id != id);
                    }
                }
                Event::Patch(value) => {
                    let data = &value["data"];
                    if let Some(id)=data["channel_id"].as_str(){self.navigation.invalidate(id);}
                    if self
                        .channel
                        .as_ref()
                        .is_some_and(|c| Some(c.id.as_str()) == data["channel_id"].as_str())
                    {
                        if value["type"] == "MESSAGE_UPDATE" {
                            if self.prefs.message_logger{if let Some(old)=self.messages.iter().find(|m|Some(m.id.as_str())==data["id"].as_str()&&crate::timeline::is_edit(m,data)).cloned(){self.log_message(old,false);}}
                            if let Some(message) = self
                                .messages
                                .iter_mut()
                                .find(|m| Some(m.id.as_str()) == data["id"].as_str())
                            {
                                if let Some(content) = data["content"].as_str() {
                                    message.content = content.into();
                                }
                                if let Some(edited) = data["edited_timestamp"].as_str() {
                                    message.edited_timestamp = Some(edited.into());
                                }
                                if let Some(items) = data.get("reactions") {
                                    if let Ok(reactions) = serde_json::from_value(items.clone()) {
                                        message.reactions = reactions;
                                    }
                                }
                                if let Some(items) = data.get("embeds") {
                                    if let Ok(embeds) = serde_json::from_value(items.clone()) {
                                        message.embeds = embeds;
                                    }
                                }
                                if let Some(items) = data.get("attachments") {
                                    if let Ok(attachments) = serde_json::from_value(items.clone()) {
                                        message.attachments = attachments;
                                    }
                                }
                            }
                        } else if value["type"] == "MESSAGE_DELETE_BULK" {
                            if let Some(ids) = data["ids"].as_array() {
                                if self.prefs.message_logger {let old:Vec<_>=self.messages.iter().filter(|m|ids.iter().any(|id|id.as_str()==Some(m.id.as_str()))).cloned().collect();for m in old{self.log_message(m,true);}}
                                self.messages.retain(|m| {
                                    !ids.iter().any(|id| id.as_str() == Some(m.id.as_str()))
                                });
                            }
                        } else {
                            self.patch_reaction(data, value["type"].as_str().unwrap_or(""));
                        }
                    }
                }
                Event::Gateway(status) => self.status = status,
                Event::Error(error) => {
                    self.picker.pending = false;
                    if self.connecting {
                        self.pending_save = None;
                        // A saved session Discord no longer accepts is forgotten so the sign-in screen shows.
                        if self.auto_login && error.contains("rejected the token") { crate::login::saved::delete(); }
                        self.auto_login = false;
                    }
                    self.connecting = false;
                    self.error = Some(error);
                }
                Event::Pins(channel, messages) => {
                    if self.channel.as_ref().is_some_and(|c| c.id == channel) {
                        self.pins = Some(messages);
                    }
                }
                Event::Dm(channel) => {
                    if let Some(existing) = self.dms.iter_mut().find(|c| c.id == channel.id) {
                        *existing = channel.clone();
                    } else {
                        self.dms.insert(0, channel.clone());
                    }
                    crate::navigation::sort_dms(&mut self.dms);
                    self.guild = None;
                    self.channels = self.dms.clone();
                    self.select_channel(channel);
                }
                Event::ReactionRefresh(channel) => {
                    if self.channel.as_ref().is_some_and(|c| c.id == channel) {
                        self.send_command(Command::History(channel, None));
                    }
                }
            }
        }
    }
    fn remember_conversation(&mut self){if self.loaded&&!self.detached{if let Some(channel)=&self.channel{self.navigation.save(&channel.id,&self.messages,self.has_older);}}}
    fn receive_message(&mut self, message: Message, count_unread: bool) {
        self.navigation.receive(message.clone());
        self.note_latest(message.channel_id.clone(),message.id.clone());
        if let Some(dm) = self.dms.iter_mut().find(|c| c.id == message.channel_id) {
            let newest = message.id.parse::<u64>().unwrap_or(0);
            let previous = dm.last_message_id.as_deref().and_then(|id| id.parse::<u64>().ok()).unwrap_or(0);
            if newest > previous {
                dm.last_message_id = Some(message.id.clone());
                crate::navigation::sort_dms(&mut self.dms);
                if self.guild.is_none() { self.channels = self.dms.clone(); }
            }
        }
        if self
            .channel
            .as_ref()
            .is_some_and(|c| c.id == message.channel_id)
        {
            // A detached window would show a gap before the new message; Jump to present reloads instead.
            if !self.detached{merge_message(&mut self.messages, message);}
        } else if count_unread {
            let count = self.unread.entry(message.channel_id).or_default();
            *count = count.saturating_add(1);
        }
    }
    fn patch_reaction(&mut self, data: &serde_json::Value, kind: &str) {
        if let Some(message) = self
            .messages
            .iter_mut()
            .find(|m| Some(m.id.as_str()) == data["message_id"].as_str())
        {
            if kind == "MESSAGE_REACTION_REMOVE_ALL" {
                message.reactions.clear();
                return;
            }
            let Ok(emoji) = serde_json::from_value::<Emoji>(data["emoji"].clone()) else {
                return;
            };
            let me = self
                .user
                .as_ref()
                .is_some_and(|u| Some(u.id.as_str()) == data["user_id"].as_str());
            if let Some(reaction) = message
                .reactions
                .iter_mut()
                .find(|r| r.emoji.route() == emoji.route())
            {
                if kind == "MESSAGE_REACTION_ADD" {
                    reaction.count += 1;
                    if me {
                        reaction.me = true;
                    }
                } else {
                    reaction.count = reaction.count.saturating_sub(1);
                    if me {
                        reaction.me = false;
                    }
                }
            } else if kind == "MESSAGE_REACTION_ADD" {
                message.reactions.push(Reaction {
                    count: 1,
                    me,
                    emoji,
                });
            }
            message.reactions.retain(|r| r.count > 0);
        }
    }
    fn login(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(preferences_bg(&self.prefs)))
            .show(ctx, |ui| {
                let width = ui.available_width();
                ui.add_space((ui.available_height() - 640.0).max(12.0) / 2.0);
                ui.horizontal(|ui| {
                    ui.add_space(((width - 640.0) / 2.0).max(20.0));
                    ui.vertical(|ui| {
                        ui.set_width((width - 40.0).min(620.0));
                        ui.horizontal(|ui| {
                            eclipse_mark(ui, 38.0);
                            ui.label(RichText::new("ECLIPSE").size(17.0).strong().color(self.accent()));
                        });
                        ui.add_space(23.0);
                        ui.label(
                            RichText::new("A little more room\nto breathe.")
                                .size(39.0)
                                .strong(),
                        );
                        ui.add_space(14.0);
                        ui.label(
                            RichText::new("An independent Discord client, written in Rust.")
                                .color(MUTED)
                                .size(16.0),
                        );
                        ui.add_space(26.0);
                        egui::Frame::NONE
                            .fill(CARD)
                            .corner_radius(12)
                            .inner_margin(18)
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                self.login_card(ui, ctx);
                            });
                        ui.add_space(13.0);
                        if ui.button("Explore offline preview").clicked() {
                            self.backend = None;
                            self.connecting = false;
                            self.error = None;
                            self.load_demo();
                        }
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new("Native Rust preview  ·  Connect to chat and call")
                                .size(12.0)
                                .color(MUTED),
                        );
                    });
                });
            });
    }
    /// Server rail and channel list share one column so the account bar can span both, like Discord.
    fn left_column(&mut self, ctx: &egui::Context) {
        let sidebar=if ctx.screen_rect().width()<900.0{224.0}else{248.0};
        egui::SidePanel::left("left-column").exact_width(70.0+sidebar).resizable(false).show_separator_line(self.compact)
            .frame(egui::Frame::NONE.fill(preferences_bg(&self.prefs)))
            .show(ctx,|ui|{
                egui::TopBottomPanel::bottom("user-panel").exact_height(panels::USER_PANEL_HEIGHT+if self.compact{0.0}else{5.0}).show_separator_line(self.compact)
                    .frame(if self.compact{egui::Frame::NONE}else{egui::Frame::NONE.inner_margin(egui::Margin{left:4,right:3,top:0,bottom:5})})
                    .show_inside(ui,|ui|self.user_panel(ui));
                self.rail(ui);
                self.sidebar(ui,sidebar);
            });
    }
    fn rail(&mut self, ui: &mut egui::Ui) {
        egui::SidePanel::left("server-rail")
            .exact_width(70.0)
            .resizable(false)
            .show_separator_line(self.compact)
            .frame(self.panel_frame(egui::Margin::symmetric(4, 5)))
            .show_inside(ui, |ui| {
                let height = ui.available_height() - 12.0;
                // Rail buttons are laid out for a 48-point column; compact mode keeps that column centred.
                let margin = if self.compact { egui::Margin::symmetric(11, 4) } else { egui::Margin::same(6) };
                self.surface().inner_margin(margin).show(ui, |ui| {
                    ui.set_min_height(height);
                    ui.spacing_mut().item_spacing.y=3.0;
                    if moon_button(ui,self.guild.is_none()).on_hover_text("Direct messages").clicked() {
                        self.navigate_home(Home::Friends);
                    }
                    if self.prefs.read_all { ui.add_space(6.); if crate::widgets::read_all(ui).clicked() { self.read_all(); } }
                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(5.0);
                    egui::ScrollArea::vertical()
                        .id_salt("guilds")
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .max_height(ui.available_height() - 70.0)
                        .show(ui, |ui| {
                            for entry in self.layout.rail(&self.guilds) {
                                match entry {
                                    RailEntry::Server(guild) => self.guild_button(ui, &guild),
                                    RailEntry::Folder {
                                        key,
                                        name,
                                        color,
                                        guilds,
                                    } => {
                                        let color = color
                                            .map(|c| {
                                                Color32::from_rgb(
                                                    (c >> 16) as u8,
                                                    (c >> 8) as u8,
                                                    c as u8,
                                                )
                                            })
                                            .unwrap_or(self.accent());
                                        let expanded = self.open_folders.contains(&key);
                                        egui::Frame::NONE
                                            .fill(color.gamma_multiply(if expanded {
                                                0.14
                                            } else {
                                                0.06
                                            }))
                                            .corner_radius(14)
                                            .show(ui, |ui| {
                                                let (rect, response) = ui.allocate_exact_size(
                                                    Vec2::splat(44.0),
                                                    egui::Sense::click(),
                                                );
                                                if expanded {
                                                    ui.painter().rect_filled(
                                                        egui::Rect::from_min_size(
                                                            rect.min + Vec2::new(12.0, 16.0),
                                                            Vec2::new(25.0, 20.0),
                                                        ),
                                                        4,
                                                        color,
                                                    );
                                                    ui.painter().rect_filled(
                                                        egui::Rect::from_min_size(
                                                            rect.min + Vec2::new(12.0, 12.0),
                                                            Vec2::new(12.0, 10.0),
                                                        ),
                                                        3,
                                                        color,
                                                    );
                                                } else {
                                                    for (i, guild) in
                                                        guilds.iter().take(4).enumerate()
                                                    {
                                                        let icon = egui::Rect::from_min_size(
                                                            rect.min
                                                                + Vec2::new(
                                                                    4.0 + (i % 2) as f32 * 19.0,
                                                                    4.0 + (i / 2) as f32 * 19.0,
                                                                ),
                                                            Vec2::splat(17.0),
                                                        );
                                                        ui.painter().rect_filled(icon, 5, CARD);
                                                        ui.painter().text(
                                                            icon.center(),
                                                            egui::Align2::CENTER_CENTER,
                                                            initials_of(&guild.name),
                                                            egui::FontId::proportional(8.0),
                                                            TEXT,
                                                        );
                                                        let url = self.guild_image_url(guild);
                                                        self.paint_image(ui, icon, url, 5);
                                                    }
                                                }
                                                if guilds.iter().any(|g| {
                                                    Some(g.id.as_str()) == self.guild.as_deref()
                                                }) {
                                                    ui.painter().line_segment(
                                                        [
                                                            rect.left_center()
                                                                + Vec2::new(-3.0, -10.0),
                                                            rect.left_center()
                                                                + Vec2::new(-3.0, 10.0),
                                                        ],
                                                        Stroke::new(3.0_f32, color),
                                                    );
                                                }
                                                if response
                                                    .on_hover_text(format!(
                                                        "{name}\n{} servers · Click to {}",
                                                        guilds.len(),
                                                        if expanded {
                                                            "collapse"
                                                        } else {
                                                            "expand"
                                                        }
                                                    ))
                                                    .clicked()
                                                {
                                                    if expanded {
                                                        self.open_folders.remove(&key);
                                                    } else {
                                                        self.open_folders.insert(key.clone());
                                                    }
                                                }
                                                if expanded {
                                                    for guild in &guilds {
                                                        self.guild_button(ui, guild);
                                                        ui.add_space(1.0);
                                                    }
                                                }
                                            });
                                    }
                                }
                                ui.add_space(1.0);
                            }
                        });
                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                        if server_button(ui, "•••", false)
                            .on_hover_text("Settings")
                            .clicked()
                        {
                            self.settings = true;
                        }
                    });
                });
            });
    }
    fn guild_image_url(&self, guild: &Guild) -> Option<String> {
        if self.preview {
            Some(format!("demo://server/{}", guild.id))
        } else {
            assets::guild_url(guild)
        }
    }
    fn paint_image(&mut self, ui: &egui::Ui, rect: egui::Rect, url: Option<String>, radius: u8) {
        self.paint_image_playing(ui,rect,url,radius,false);
    }
    /// `always` keeps an animated image playing without hover (the selected server icon).
    fn paint_image_playing(&mut self, ui: &egui::Ui, rect: egui::Rect, url: Option<String>, radius: u8, always: bool) {
        if ui.is_rect_visible(rect) {
            if let Some(id) = url.and_then(|url| self.images.texture_playing(&url,rect, ui.ctx(), always)) {
                ui.painter().rect_filled(rect, radius, CARD);
                egui::Image::new((id, rect.size()))
                    .corner_radius(radius)
                    .paint_at(ui, rect);
            }
        }
    }
    fn user_avatar(
        &mut self,
        ui: &mut egui::Ui,
        user: &User,
        member_avatar: Option<&str>,
        size: f32,
    ) {
        self.avatar_with_status(ui,user,member_avatar,size,true);
    }
    /// Chat avatars omit the presence badge; lists and profiles keep it.
    fn avatar_with_status(&mut self,ui:&mut egui::Ui,user:&User,member_avatar:Option<&str>,size:f32,show_status:bool){
        let response=self.paint_avatar(ui,user,member_avatar,size,show_status);
        response.context_menu(|ui|self.user_menu(ui,user));if response.clicked(){self.toggle_profile_at(user,response.rect); }
    }
    /// A clickable avatar with decoration and (optionally) presence badge; callers decide what a click does.
    fn paint_avatar(&mut self,ui:&mut egui::Ui,user:&User,member_avatar:Option<&str>,size:f32,show_status:bool)->egui::Response{
        let (rect,response) = avatar_response(ui, user.name(), size,egui::Sense::click());
        let url = if self.preview {
            Some(format!("demo://user/{}", user.id))
        } else {
            assets::avatar_url(user, self.guild.as_deref(), member_avatar)
        };
        self.paint_image(ui, rect, url, (size / 2.0) as u8);
        let status = self.presences.get(&user.id, self.guild.as_deref());
        let mut decorated=user.clone();if self.guild.as_deref()==Some(&self.server.id){if let Some(decoration)=self.server.members.get(&user.id).and_then(|m|m.avatar_decoration_data.clone()){decorated.avatar_decoration_data=Some(decoration);}}
        crate::identity::paint_art_playing(ui,&mut self.images,rect.expand(size*0.1),crate::identity::decoration(&decorated),0);
        if show_status{crate::presence::badge(ui, rect, status);}
        if show_status{response.on_hover_text(status.label())}else{response}
    }
    fn guild_button(&mut self, ui: &mut egui::Ui, guild: &Guild) {
        let selected = self.guild.as_deref() == Some(&guild.id);
        let (_,response)=ui.allocate_exact_size(Vec2::splat(44.0),egui::Sense::click());
        let hover=ui.ctx().animate_bool_with_time(egui::Id::new(("guild-hover",&guild.id)),response.hovered(),if self.prefs.reduced_motion{0.0}else{0.12});
        let rect=response.rect.expand(2.0*hover);
        if self.preview&&self.preview_gesture==Some("server")&&selected{ui.ctx().data_mut(|d|d.insert_temp(egui::Id::new("preview-gesture-point"),response.rect.center()));}
        let radius=if selected{14}else{22};
        ui.painter().rect_filled(rect,radius,if selected{self.accent()}else{CARD});
        ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,initials_of(&guild.name),egui::FontId::proportional(14.0),if selected{SIDE}else{MUTED});
        self.paint_image_playing(
            ui,
            rect,
            self.guild_image_url(guild),
            radius,
            selected,
        );
        if selected {
            ui.painter().rect_stroke(
                rect,
                radius,
                Stroke::new(2.0_f32, self.accent()),
                egui::StrokeKind::Inside,
            );
        }
        response.context_menu(|ui|{if ui.button("Server settings").clicked(){self.choose_guild(guild);self.open_server_settings();ui.close();}if ui.button("Copy server ID").clicked(){ui.ctx().copy_text(guild.id.clone());ui.close();}});
        if response.on_hover_text(&guild.name).clicked() && !selected {
            self.choose_guild(guild);
        }
    }
    fn choose_guild(&mut self, guild: &Guild) {
        self.home=Home::Chat;
        self.profile=None;
        self.remember_conversation();
        self.subscribe(Some(&guild.id), None);
        self.guild = Some(guild.id.clone());
        self.channel = None;
        self.messages.clear();
        self.search.clear();
        self.channel_filter.clear();
        if self.preview {
            self.channels = demo().2;
            for channel in &mut self.channels {
                channel.guild_id = Some(guild.id.clone());
            }
            if let Some(channel) = self.channels.iter().find(|c| c.is_text()).cloned() {
                self.select_channel(channel);
            }
        } else {
            self.channels=self.navigation.channels(&guild.id).unwrap_or_default();
            let last=self.navigation.last_channel(&guild.id);
            if let Some(channel)=self.channels.iter().find(|c|Some(c.id.as_str())==last.as_deref()&&c.is_text()).or_else(||self.channels.iter().find(|c|c.is_text())).cloned(){self.select_channel(channel);}
            self.send_command(Command::Channels(guild.id.clone()));
        }
        self.load_server(&guild.id);
    }
    fn profile_patch(&mut self, kind: &str, data: &serde_json::Value) {
        let id = data["id"].as_str().unwrap_or("");
        if kind == "GUILD_UPDATE" {
            if let Some(guild) = self.guilds.iter_mut().find(|g| g.id == id) {
                if let Some(icon) = data.get("icon") {
                    guild.icon = icon.as_str().map(str::to_owned);
                }
                if let Some(name) = data["name"].as_str() {
                    guild.name = name.into();
                }
            }
        } else {
            let patch = |user: &mut User| {
                if user.id != id {
                    return;
                }
                let mut value=serde_json::to_value(&*user).unwrap_or_default();
                if let (Some(target),Some(patch))=(value.as_object_mut(),data.as_object()){for(key,value)in patch{target.insert(key.clone(),value.clone());}}
                if let Ok(updated)=serde_json::from_value(value){*user=updated;}
                if let Some(avatar) = data.get("avatar") {
                    user.avatar = avatar.as_str().map(str::to_owned);
                }
                if let Some(name) = data["username"].as_str() {
                    user.username = name.into();
                }
                if let Some(name) = data.get("global_name") {
                    user.global_name = name.as_str().map(str::to_owned);
                }
            };
            if let Some(user) = &mut self.user {
                patch(user);
            }
            for channel in self.dms.iter_mut().chain(self.channels.iter_mut()) {
                for user in &mut channel.recipients {
                    patch(user);
                }
            }
            for message in &mut self.messages {
                patch(&mut message.author);
                if let Some(reply) = &mut message.referenced_message {
                    patch(&mut reply.author);
                }
            }
        }
    }
    fn dm_button(
        &mut self,
        ui: &mut egui::Ui,
        channel: &Channel,
        selected: bool,
        unread: usize,
    ) -> egui::Response {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), egui::Sense::click());
        if selected || response.hovered() {
            ui.painter().rect_filled(rect, 20, CARD);
        }
        if let Some(user)=channel.recipients.first().filter(|_|channel.kind==1){crate::identity::paint_art_playing(ui,&mut self.images,rect,crate::identity::nameplate(user),8);}
        let icon = egui::Rect::from_min_size(rect.min + Vec2::new(7.0, 5.0), Vec2::splat(30.0));
        let label = channel.label();
        ui.painter()
            .circle_filled(icon.center(), 15.0, name_color(&label).gamma_multiply(0.3));
        ui.painter().text(
            icon.center(),
            egui::Align2::CENTER_CENTER,
            initials_of(&label),
            egui::FontId::proportional(11.0),
            TEXT,
        );
        let url = if self.preview {
            Some(format!(
                "demo://user/{}",
                channel
                    .recipients
                    .first()
                    .map(|u| u.id.as_str())
                    .unwrap_or(&channel.id)
            ))
        } else {
            assets::channel_url(channel).or_else(|| {
                channel
                    .recipients
                    .first()
                    .and_then(|user| assets::avatar_url(user, None, None))
            })
        };
        self.paint_image(ui, icon, url, 15);
        if let Some(user)=channel.recipients.first().filter(|_|channel.kind==1){crate::identity::paint_art_playing(ui,&mut self.images,icon.expand(3.),crate::identity::decoration(user),0);}
        let text = if unread > 0 {
            format!("{label}  {unread}")
        } else {
            label
        };
        crate::widgets::row_text(ui, rect, 45.0, &text, if selected { TEXT } else { MUTED });
        if let Some(user) = channel.recipients.first().filter(|_| channel.kind == 1) {
            crate::presence::badge(ui, icon, self.presences.get(&user.id, None));
        }
        response.on_hover_text(format!(
            "{} · {}",
            channel.label(),
            channel
                .recipients
                .first()
                .filter(|_| channel.kind == 1)
                .map(|user| self.presences.get(&user.id, None).label())
                .unwrap_or("Group conversation")
        ))
    }
    fn sidebar(&mut self, ui: &mut egui::Ui, width: f32) {
        egui::SidePanel::left("channel-sidebar")
            .exact_width(width)
            .resizable(false)
            .show_separator_line(false)
            .frame(self.panel_frame(egui::Margin::symmetric(3, 5)))
            .show_inside(ui, |ui| {
                let spotify_height=if self.spotify.as_ref().is_some_and(|s|s.track.available){155.}else{0.};
                let call_height=if self.calls.channel().is_some(){80.0}else{0.0};
                let channel_height = (ui.available_height()-spotify_height-call_height).max(230.0);
                self.surface().inner_margin(self.pad(10)).show(ui, |ui| {
                    let mut reset_dm_scroll = false;
                    ui.set_min_height(channel_height - 20.0);
                    ui.set_min_width(ui.available_width());
                    let name = self
                        .guild
                        .as_ref()
                        .and_then(|id| self.guilds.iter().find(|g| &g.id == id))
                        .map(|g| g.name.clone())
                        .unwrap_or("Direct messages".into());
                    if let Some(guild)=self.guilds.iter().find(|g|Some(g.id.as_str())==self.guild.as_deref()).cloned(){
                        let(rect,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),38.),egui::Sense::hover());
                        let response=ui.interact(rect,egui::Id::new("server-header-click"),egui::Sense::click());
                        if response.hovered(){ui.painter().rect_filled(response.rect,10,CARD);}
                        let icon=egui::Rect::from_min_size(response.rect.min+Vec2::new(4.,7.),Vec2::splat(24.));
                        ui.painter().rect_filled(icon,7,CARD);ui.painter().text(icon.center(),egui::Align2::CENTER_CENTER,initials_of(&name),egui::FontId::proportional(9.),TEXT);
                        self.paint_image(ui,icon,self.guild_image_url(&guild),7);
                        let text_rect=egui::Rect::from_min_max(icon.right_top()+Vec2::new(10.,-7.),response.rect.right_bottom()-Vec2::new(26.,0.));
                        let mut job=egui::text::LayoutJob::simple(name.clone(),egui::FontId::proportional(17.),TEXT,text_rect.width());job.wrap.max_rows=1;job.wrap.break_anywhere=true;let galley=ui.fonts(|fonts|fonts.layout_job(job));
                        ui.painter().with_clip_rect(text_rect).galley(egui::pos2(text_rect.left(),response.rect.center().y-galley.size().y/2.),galley,TEXT);
                        ui.painter().text(response.rect.right_center()-Vec2::new(12.,0.),egui::Align2::CENTER_CENTER,"⌄",egui::FontId::proportional(15.),MUTED);
                        egui::Popup::menu(&response).id(egui::Id::new("server-header-menu")).show(|ui|{ui.set_min_width(225.);ui.strong(&name);ui.separator();
                            if ui.button("Server Settings").clicked(){self.open_server_settings();ui.close();}
                            if ui.button("Members & Roles").clicked(){self.open_server_settings();self.server_page="Members".into();self.load_server_page();ui.close();}
                            if ui.button("Notification Settings").clicked(){self.open_server_settings();self.server_page="Notifications".into();self.load_server_page();ui.close();}
                        });
                    }else{
                        // Conversation search at the top, like Discord. The placeholder clears while focused and
                        // returns when focus leaves an empty box.
                        let id=egui::Id::new("dm-conversation-search");
                        let focused=ui.memory(|m|m.has_focus(id));
                        ui.scope(|ui|{ui.visuals_mut().extreme_bg_color=CARD;
                            if ui.add(egui::TextEdit::singleline(&mut self.channel_filter).id(id).hint_text(RichText::new(if focused{""}else{"Find A Conversation"}).size(15.)).horizontal_align(egui::Align::Center).desired_width(f32::INFINITY).margin(Vec2::new(9.0,7.0))).changed() {
                                self.dm_visible = DM_PAGE_SIZE;
                                reset_dm_scroll = true;
                            }
                        });
                    }
                    ui.add_space(if self.guild.is_some(){8.0}else{4.0});
                    ui.separator();
                    ui.add_space(if self.guild.is_some(){4.0}else{0.0});
                    if self.guild.is_none(){
                        ui.scope(|ui|{ui.spacing_mut().item_spacing.y=1.0;
                            for(home,label)in[(Home::Friends,"Friends"),(Home::Nitro,"Nitro"),(Home::Shop,"Shop"),(Home::Quests,"Quests")]{if crate::widgets::home_row(ui,label,self.home==home,if self.home==home{TEXT}else{MUTED}).clicked(){self.navigate_home(home);}}
                        });
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Direct messages").size(12.0).color(MUTED).strong());
                            let plus=ui.small_button("+").on_hover_text("Find a friend or server member");
                            if plus.clicked(){self.open_dm_picker(plus.rect);}
                        });ui.add_space(5.0);
                    }
                    let filter = self.channel_filter.to_lowercase();
                    let matching: Vec<_> = self.channels.iter().filter(|c| {
                        c.guild_id.as_deref().is_none_or(|id| Some(id) == self.guild.as_deref())
                            && c.label().to_lowercase().contains(&filter)
                    }).collect();
                    let limit = if self.guild.is_none() { self.dm_visible } else { matching.len() };
                    let has_more = self.guild.is_none() && matching.len() > limit;
                    let channels: Vec<_> = matching.into_iter().take(limit).cloned().collect();
                    let mut scroll = egui::ScrollArea::vertical()
                        .id_salt(("channels", &self.guild))
                        .max_height((channel_height-if self.guild.is_some(){85.0}else{229.0}).max(70.0));
                    if reset_dm_scroll { scroll = scroll.vertical_scroll_offset(0.0); }
                    scroll.show_viewport(ui, |ui, viewport| {
                            if self.guild.is_some(){ui.spacing_mut().item_spacing.y=3.0;}
                            for channel in channels {
                                let label = channel.label();
                                if channel.kind == 4 {
                                    ui.add_space(10.0);
                                    ui.label(RichText::new(label).size(12.0).color(MUTED).strong());
                                    ui.add_space(2.0);
                                    continue;
                                }
                                let selected =
                                    self.channel.as_ref().is_some_and(|c| c.id == channel.id);
                                let prefix = if channel.is_text() {
                                    if self.guild.is_none() {
                                        "@"
                                    } else {
                                        "#"
                                    }
                                } else {
                                    "○"
                                };
                                let unread = self.unread.get(&channel.id).copied().unwrap_or(0);
                                let text = format!(
                                    "{prefix}   {label}{}",
                                    if unread > 0 {
                                        format!("   {unread}")
                                    } else {
                                        String::new()
                                    }
                                );
                                let response = if self.guild.is_none() {
                                    self.dm_button(ui, &channel, selected, unread)
                                } else {
                                    crate::widgets::channel_row(
                                        ui,
                                        &text,
                                        selected,
                                        if selected { TEXT } else { MUTED },
                                    )
                                };
                                response.context_menu(|ui|self.channel_menu(ui,&channel));
                                if matches!(channel.kind,2|13) {self.voice_drop_target(ui,&response,&channel);}
                                if selected {
                                    let rect = response.rect;
                                    ui.painter().rect_filled(
                                        egui::Rect::from_min_size(
                                            egui::pos2(rect.left(), rect.center().y - 8.0),
                                            Vec2::new(3.0, 16.0),
                                        ),
                                        2,
                                        self.accent(),
                                    );
                                }
                                self.voice_member_row(ui, &channel);
                                if response.clicked() {
                                    if channel.is_text() {
                                        self.select_channel(channel.clone());
                                    } else if channel.kind == 2 {
                                        self.start_call(&channel, false);
                                    } else {
                                        self.error = Some(
                                            "Stage and forum browsing are not available yet."
                                                .into(),
                                        );
                                    }
                                }
                            }
                            if has_more && viewport.min.y > 0.0 && viewport.max.y >= ui.min_rect().height() - 60.0 {
                                self.dm_visible = self.dm_visible.saturating_add(DM_PAGE_SIZE);
                                ui.ctx().request_repaint();
                            }
                            if self.channels.is_empty() {
                                ui.label(
                                    RichText::new(if self.guild.is_none() {
                                        "No direct messages yet."
                                    } else {
                                        "Loading channels…"
                                    })
                                    .color(MUTED),
                                );
                            }
                        });
                });
                if call_height>0.0||spotify_height>0.0{ui.add_space(10.0);}
                self.voice_connection_card(ui);
                self.spotify_card(ui);
            });
    }
    fn conversation_header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("conversation-header")
            .exact_height(if self.compact { 48.0 } else { 60.0 })
            .show_separator_line(self.compact)
            .frame(self.panel_frame(egui::Margin::symmetric(3, 5)))
            .show(ctx, |ui| {
                self.surface()
                    .inner_margin(egui::Margin::symmetric(self.pad(12), self.pad(8)))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(if self.guild.is_some() { "#" } else { "@" })
                                    .size(25.0)
                                    .color(MUTED),
                            );
                            let label = self
                                .channel
                                .as_ref()
                                .map(Channel::label)
                                .unwrap_or("Your conversations".into());
                            ui.label(RichText::new(label).size(18.0).strong());
                            if self.calls.active()&&ui.button("Call view").clicked(){self.calls.chat=false;}
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    use crate::widgets::{header_icon,HeaderIcon};
                                    // Room for the green update button, which floats in the top right.
                                    if self.updater.later(){ui.add_space(40.0);}
                                    if header_icon(ui,HeaderIcon::People,if self.show_members{TEXT}else{MUTED},if self.show_members{"Hide member list"}else{"Show member list"}).clicked()
                                    {
                                        self.show_members = !self.show_members;self.prefs.members=self.show_members;
                                    }
                                    if self.channel.as_ref().is_some_and(|c| c.guild_id.is_none()) {
                                        if ui
                                            .button("Video")
                                            .on_hover_text("Start a video call")
                                            .clicked()
                                        {
                                            if let Some(channel) = self.channel.clone() {
                                                self.start_call(&channel, true);
                                            }
                                        }
                                        if ui
                                            .button("Call")
                                            .on_hover_text("Start a voice call")
                                            .clicked()
                                        {
                                            if let Some(channel) = self.channel.clone() {
                                                self.start_call(&channel, false);
                                            }
                                        }
                                    }
                                    // One search filters both messages and the member list.
                                    if ui.available_width()>400.0{ui.scope(|ui| {
                                        ui.visuals_mut().extreme_bg_color = CARD;
                                        let hint=self.guild.as_ref().and_then(|id|self.guilds.iter().find(|g|&g.id==id)).map(|g|format!("Search {}",g.name)).unwrap_or_else(||"Search this conversation".into());
                                        ui.add_sized(
                                            [205.0, 30.0],
                                            egui::TextEdit::singleline(&mut self.search)
                                                .hint_text(hint)
                                                .clip_text(true)
                                                .margin(Vec2::new(10.0, 6.0)),
                                        );
                                    });}
                                    let pin=header_icon(ui,HeaderIcon::Pin,if self.pins_anchor.is_some(){TEXT}else{MUTED},"Pinned messages");
                                    if pin.clicked()&&self.pins_anchor.take().is_none() {
                                        self.pins_anchor=Some(pin.rect);self.pins_just_opened=true;self.pins=None;
                                        if self.preview {
                                            self.pins = Some(
                                                self.messages
                                                    .iter()
                                                    .filter(|m| m.pinned)
                                                    .cloned()
                                                    .collect(),
                                            );
                                        } else if let Some(channel) = &self.channel {
                                            self.send_command(Command::Pins(channel.id.clone()));
                                        }
                                    }
                                    if let Some(topic) =
                                        self.channel.as_ref().and_then(|c| c.topic.as_deref())
                                    {
                                        ui.add_sized(
                                            [(ui.available_width() - 15.0).max(10.0), 25.0],
                                            egui::Label::new(
                                                RichText::new(topic).size(12.0).color(MUTED),
                                            )
                                            .truncate(),
                                        );
                                    }
                                },
                            );
                        });
                    });
            });
    }

    fn members(&mut self,ctx:&egui::Context){
        if !self.show_members||ctx.screen_rect().width()<1000.0{return;}
        let mut members:HashMap<String,crate::community::Member>=if self.guild.as_deref()==Some(&self.server.id){self.server.members.clone()}else{HashMap::new()};
        for message in &self.messages{members.entry(message.author.id.clone()).or_insert_with(||crate::community::Member{user:message.author.clone(),avatar:message.member.as_ref().and_then(|m|m.avatar.clone()),roles:message.member.as_ref().map(|m|m.roles.clone()).unwrap_or_default(),nick:message.member.as_ref().and_then(|m|m.nick.clone()),..Default::default()});}
        if self.guild.is_none(){if let Some(channel)=&self.channel{for user in &channel.recipients{members.entry(user.id.clone()).or_insert_with(||crate::community::Member{user:user.clone(),..Default::default()});}}}
        let group=|member:&crate::community::Member|{let offline=matches!(self.presences.get(&member.user.id,self.guild.as_deref()),crate::presence::Status::Offline);let role=self.server.roles.iter().filter(|r|r.hoist&&member.roles.contains(&r.id)).max_by_key(|r|r.position);if offline{(i32::MAX,"Offline".to_string())}else if let Some(role)=role{(-role.position,role.name.clone())}else{(0,"Online".to_string())}};
        let mut members:Vec<_>=members.into_values().collect();members.sort_by_key(|m|(group(m).0,m.name().to_lowercase()));
        egui::SidePanel::right("members").exact_width(238.).resizable(false).show_separator_line(self.compact).frame(self.panel_frame(egui::Margin::symmetric(3,3))).show(ctx,|ui|{
            let height=ui.available_height()-20.;self.surface().inner_margin(self.pad(10)).show(ui,|ui|{ui.set_min_height(height);ui.set_min_width(ui.available_width());
                egui::ScrollArea::vertical().id_salt("member-list").max_height(height-20.).show(ui,|ui|{ui.spacing_mut().item_spacing.y=4.0;let mut previous=String::new();let search=self.search.to_lowercase();for member in members.iter().filter(|m|m.name().to_lowercase().contains(&search)||m.user.username.to_lowercase().contains(&search)).cloned().collect::<Vec<_>>(){
                    let offline=matches!(self.presences.get(&member.user.id,self.guild.as_deref()),crate::presence::Status::Offline);let role=self.server.roles.iter().filter(|r|r.hoist&&member.roles.contains(&r.id)).max_by_key(|r|r.position);let group=if offline{"OFFLINE".into()}else{role.map(|r|r.name.to_uppercase()).unwrap_or("ONLINE".into())};if group!=previous{ui.add_space(8.);ui.label(RichText::new(&group).size(10.).strong().color(MUTED));previous=group;}
                    let color=if self.prefs.role_colors{self.server.color(&member.user.id).unwrap_or(TEXT)}else{TEXT};
                    let response=egui::Frame::NONE.fill(Color32::TRANSPARENT).corner_radius(7).inner_margin(4).show(ui,|ui|{ui.spacing_mut().item_spacing.y=2.0;ui.set_min_width(ui.available_width());ui.set_min_height(34.0);let plate=egui::Rect::from_min_size(ui.cursor().min,Vec2::new(ui.available_width(),34.));crate::identity::paint_art_playing(ui,&mut self.images,plate,crate::identity::nameplate(&member.user),7);ui.horizontal(|ui|{self.user_avatar(ui,&member.user,member.avatar.as_deref(),30.);ui.vertical(|ui|{ui.set_max_width(ui.available_width());let name=crate::identity::name(ui,&member.user,member.name(),14.,color);if name.clicked(){self.toggle_profile_at(&member.user,name.rect);}ui.label(RichText::new(self.presences.get(&member.user.id,self.guild.as_deref()).label()).size(10.).color(MUTED));});});}).response.interact(egui::Sense::click());response.context_menu(|ui|self.user_menu(ui,&member.user));if response.clicked(){self.toggle_profile_at(&member.user,response.rect);}
                }if self.guild.is_some()&&self.server.has_more&&self.server.members.len()<crate::community::MEMBER_LIMIT&&ui.button("Load more members").clicked(){let guild=self.server.id.clone();let after=self.server.cursor.clone().unwrap_or_default();self.request_feature(&format!("members:{guild}"),format!("/guilds/{guild}/members?limit=100&after={after}"));}});
            });
        });
    }
    fn conversation(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().frame(self.panel_frame(egui::Margin::symmetric(3,3)))
            .show(ctx,|ui| {
                let height=ui.available_height()-20.0;
                self.surface().inner_margin(self.pad(10)).show(ui,|ui| {
                    ui.set_min_height(height);ui.set_min_width(ui.available_width());
                    let Some(channel)=self.channel.clone()else{
                        ui.add_space(ui.available_height()*0.3);ui.vertical_centered(|ui| {
                            eclipse_mark(ui,52.0);ui.add_space(16.0);ui.heading("Pick a conversation");
                            ui.label(RichText::new("Your servers and direct messages are on the left.").color(MUTED));
                        });return;
                    };
                    if self.preview{
                        egui::Frame::NONE.fill(self.accent()).corner_radius(8).inner_margin(egui::Margin::symmetric(12,9)).show(ui,|ui| {
                            ui.set_min_width(ui.available_width());
                            ui.label(RichText::new("Offline preview  ·  Sample messages. Nothing is sent to Discord.").size(12.0).strong().color(SIDE));
                        });ui.add_space(8.0);
                    }
                    if let Some(error)=self.error.clone(){
                        ui.horizontal_wrapped(|ui| {ui.colored_label(Color32::from_rgb(240,132,140),error);if ui.small_button("Dismiss").clicked(){self.error=None;}});ui.add_space(8.0);
                    }
                    if self.detached{
                        egui::Frame::NONE.fill(CARD).corner_radius(8).inner_margin(egui::Margin::symmetric(12,6)).show(ui,|ui|{
                            ui.set_min_width(ui.available_width());
                            ui.horizontal(|ui|{ui.label(RichText::new("You're viewing older messages").size(12.0).color(MUTED));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if ui.small_button("Jump to present").clicked(){self.messages.clear();self.detached=false;self.loaded=false;self.highlight=None;self.send_command(Command::History(channel.id.clone(),None));}});});
                        });ui.add_space(8.0);
                    }
                    let file_height=if self.files.contains_key(&channel.id){32.0}else{0.0};
                    let scroll_height=(ui.available_height()-110.0-file_height).max(60.0);
                    egui::ScrollArea::vertical().id_salt(("history",&channel.id)).stick_to_bottom(true).max_height(scroll_height).auto_shrink([false,false]).show(ui,|ui| {
                        ui.spacing_mut().item_spacing.y = if self.compact { 0.0 } else { 2.0 };
                        if self.has_older&&!self.preview{
                            let enabled=self.messages.len()<HISTORY_LIMIT;
                            if ui.add_enabled(enabled,egui::Button::new(if enabled{"Load older messages"}else{"200-message memory limit reached"})).clicked(){self.send_command(Command::History(channel.id.clone(),self.messages.front().map(|m|m.id.clone())));}
                        }
                        if self.messages.is_empty(){ui.add_space(30.0);ui.heading(if self.loaded{"The conversation starts here."}else{"Loading messages…"});}
                        ui.add_space(8.0);
                        let search=self.search.to_lowercase();
                        let messages=crate::timeline::messages(&self.messages,&self.logs,&channel.id);
                        let now=ui.input(|i|i.time);
                        let mut previous_visible = None;
                        for (index, message) in messages.iter().enumerate(){
                            if !search.is_empty() && !message.content.to_lowercase().contains(&search) && !message.author.name().to_lowercase().contains(&search) { continue; }
                            let grouped = index > 0 && previous_visible == Some(index - 1) && crate::timeline::grouped(&messages[index - 1], message);
                            if !grouped && previous_visible.is_some() { ui.add_space(if self.compact { 3.0 } else { 10.0 }); }
                            previous_visible = Some(index);
                            let bg=ui.painter().add(egui::Shape::Noop);
                            let rect=ui.scope(|ui|if grouped { self.message_ui_grouped(ui,message,true); } else { self.message_ui(ui,message); }).response.rect;
                            if self.jump_to.as_ref().is_some_and(|(id,_)|*id==message.id){
                                ui.scroll_to_rect(rect,Some(egui::Align::Center));self.highlight=Some((message.id.clone(),now));
                                self.jump_to=self.jump_to.take().and_then(|(id,frames)|(frames>0).then(||(id,frames-1)));
                            }
                            if let Some((_,start))=self.highlight.as_ref().filter(|(id,_)|*id==message.id){
                                let fade=1.0-((now-start)/2.5) as f32;
                                if fade>0.0{let [r,g,b,_]=self.accent().to_array();ui.painter().set(bg,egui::Shape::rect_filled(rect.expand2(Vec2::new(6.0,3.0)),6,Color32::from_rgba_unmultiplied(r,g,b,(fade*55.0) as u8)));ui.ctx().request_repaint();}
                            }
                        }
                        ui.add_space(12.0);
                    });
                    ui.add_space(6.0);self.composer(ui,ctx,&channel);
                });
            });
    }

    fn message_ui(&mut self, ui: &mut egui::Ui, message: &Message) {
        self.message_ui_grouped(ui, message, false);
    }
    fn message_ui_grouped(&mut self, ui: &mut egui::Ui, message: &Message, grouped: bool) {
        let deleted=self.logs.iter().any(|e|e.deleted&&e.message.id==message.id&&e.message.channel_id==message.channel_id);
        let versions:Vec<_>=self.logs.iter().filter(|e|!e.deleted&&e.message.id==message.id&&e.message.channel_id==message.channel_id).map(|e|e.message.content.clone()).collect();
        let logged=deleted||!versions.is_empty();
        let highlight = self.user.as_ref().is_some_and(|u| {
            !self.prefs.quiet_mentions&&(message.content.contains(&format!("<@{}>", u.id))
                || message.content.contains(&format!("@{}", u.name())))
        });
        let response = ui.scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()),|ui| {egui::Frame::NONE
            .fill(Color32::TRANSPARENT)
            .corner_radius(19)
            .inner_margin(egui::Margin::symmetric(8, if grouped { 0 } else if self.compact { 2 } else { 4 }))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                if let Some(reply) = &message.referenced_message {
                    ui.horizontal(|ui| {
                        ui.add_space(49.0);
                        self.avatar_with_status(
                            ui,
                            &reply.author,
                            reply.member.as_ref().and_then(|m| m.avatar.as_deref()),
                            16.0,
                            false,
                        );
                        ui.label(
                            RichText::new(format!("@{}", reply.author.name()))
                                .size(12.0)
                                .color(self.accent()),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(self.readable(reply,&crate::message_media::visible_content(reply,self.prefs.images)).chars().take(90).collect::<String>())
                                    .size(12.0)
                                    .color(MUTED),
                            )
                            .truncate(),
                        );
                    });
                    ui.add_space(if self.compact { 0.0 } else { 2.0 });
                }
                ui.horizontal_top(|ui| {
                    if grouped {
                        let (_, response) = ui.allocate_exact_size(Vec2::new(40.0, 16.0), egui::Sense::hover());
                        response.on_hover_text(self.message_clock.label(&message.timestamp));
                    } else {
                        self.avatar_with_status(
                            ui,
                            &message.author,
                            message.member.as_ref().and_then(|m| m.avatar.as_deref()),
                            40.0,
                            false,
                        );
                    }
                    ui.add_space(5.0);
                    // Plain chat text keeps server chats, group DMs and DMs visually consistent.
                    ui.vertical(|ui| {
                    egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(0, if grouped || self.compact { 0 } else { 3 }))
                        .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        if self.compact { ui.spacing_mut().item_spacing.y = 0.0; }
                        if !grouped { ui.horizontal_wrapped(|ui| {
                            let author_response=crate::identity::name(ui,&message.author,message.author.name(),15.,if self.prefs.role_colors{self.server.color(&message.author.id).unwrap_or_else(||name_color(message.author.name()))}else{TEXT});
                            self.guild_tag_chip(ui,&message.author);
                            author_response.context_menu(|ui|self.user_menu(ui,&message.author));if author_response.clicked(){self.toggle_profile_at(&message.author,author_response.rect);}
                            let time = self.message_clock.label(&message.timestamp);
                            ui.label(
                                RichText::new(time)
                                    .size(11.0)
                                    .color(MUTED.gamma_multiply(0.65)),
                            );
                            if deleted {ui.label(RichText::new("Deleted").size(10.).color(LOG_RED));}
                            else if message.edited_timestamp.is_some() || !versions.is_empty() {ui.label(RichText::new("Edited").size(10.).color(if logged{LOG_RED}else{MUTED}));}
                        }); }
                        else if deleted || message.edited_timestamp.is_some() || !versions.is_empty() {
                            ui.label(RichText::new(if deleted { "Deleted" } else { "Edited" }).size(10.).color(if logged { LOG_RED } else { MUTED }));
                        }
                        if self.edit.as_ref().is_some_and(|(id,_)|*id==message.id) {
                            self.inline_editor(ui,message);
                        } else if !message.content.is_empty() {
                            let display=self.prefs.display(&self.readable(message,&crate::message_media::visible_content(message,self.prefs.images)));
                            ui.scope(|ui|{if logged{ui.visuals_mut().override_text_color=Some(LOG_RED);}
                                for response in crate::message_media::body(ui, &mut self.images, &display){if deleted{response.context_menu(|ui|{if ui.button("Copy deleted text").clicked(){ui.ctx().copy_text(message.content.clone());ui.close();}});}else{response.context_menu(|ui|self.message_menu(ui,message));self.message_click(&response,message);}}
                            });
                            if !versions.is_empty(){egui::CollapsingHeader::new(RichText::new(format!("Previous edit{}",if versions.len()==1{""}else{"s"})).size(11.).color(LOG_RED)).id_salt(("edit-history",&message.id)).default_open(true).show(ui,|ui|{ui.visuals_mut().override_text_color=Some(LOG_RED);for text in versions.iter().rev().take(5){crate::message_media::body(ui,&mut self.images,text);}if versions.len()>5{ui.weak(format!("{} earlier edits retained this session",versions.len()-5));}});}
                            if self.prefs.show_usernames && !grouped {ui.weak(format!("@{}",message.author.username));}
                            if self.prefs.images&&message.embeds.is_empty() {
                                for url in crate::message_media::direct_images(&message.content) {
                                    crate::message_media::picture(ui,&mut self.images,&url,None,None,true);
                                }
                            }
                        }
                        for attachment in &message.attachments {
                            let inline=self.prefs.images&&assets::public_url(&attachment.url);
                            if inline {
                                crate::message_media::picture(
                                    ui,
                                    &mut self.images,
                                    &attachment.url,
                                    None,
                                    None,
                                    true,
                                );
                            }
                            if !inline && safe_link(&attachment.url) {
                                egui::Frame::NONE
                                    .fill(CARD)
                                    .corner_radius(7)
                                    .inner_margin(10)
                                    .show(ui, |ui| {
                                        ui.push_id(&attachment.id, |ui| {
                                            ui.hyperlink_to(
                                                format!(
                                                    "↓  {}  ·  {:.1} KB",
                                                    attachment.filename,
                                                    attachment.size as f64 / 1024.0
                                                ),
                                                &attachment.url,
                                            )
                                        });
                                    });
                            }
                        }
                        for embed in &message.embeds {
                            if self.prefs.images&&crate::message_media::media_embed(embed) {
                                let animated=embed.video.as_ref().filter(|v|embed.kind=="gifv"||v.url.as_ref().is_some_and(|u|u.contains(".gif")));
                                if let Some(image)=animated.or(embed.image.as_ref()).or(embed.thumbnail.as_ref()) {if let Some(url)=image.url.as_deref().filter(|u|assets::public_url(u)).or(image.proxy_url.as_deref()){crate::message_media::picture(ui,&mut self.images,url,image.width,image.height,true);}}
                            } else if embed.title.is_some()||embed.description.is_some()||embed.provider.is_some()||embed.author.is_some()||!embed.fields.is_empty() {
                                ui.push_id(("embed",&message.id,embed.url.as_deref()),|ui|crate::message_media::embed_card(ui,&mut self.images,embed,self.prefs.images));
                            }
                        }
                        if !deleted && !message.reactions.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                for reaction in &message.reactions {
                                    let label = format!(
                                        "{}  {}",
                                        reaction.emoji.name.as_deref().unwrap_or("?"),
                                        reaction.count
                                    );
                                    if ui
                                        .add(egui::Button::new(label).selected(reaction.me))
                                        .clicked()
                                    {
                                        self.react(message, reaction.emoji.route(), reaction.me);
                                    }
                                }
                            });
                        }
                    });
                    });
                });
            });}).response;
        if crate::message_media::viewer_needs_caption(ui.ctx()) {
            let avatar=assets::avatar_url(&message.author,self.guild.as_deref(),message.member.as_ref().and_then(|m|m.avatar.as_deref()));
            let time=self.message_clock.label(&message.timestamp);
            crate::message_media::caption_viewer(ui.ctx(),message.author.name(),&time,avatar);
        }
        if self.preview&&self.preview_gesture==Some("message")&&self.user.as_ref().is_some_and(|u|u.id==message.author.id){ui.ctx().data_mut(|d|d.insert_temp(egui::Id::new("preview-gesture-point"),response.rect.center()));}
        if highlight {
            let rect = response.rect;
            ui.painter().rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(rect.left() + 2.0, rect.top() + 8.0),
                    Vec2::new(3.0, (rect.height() - 16.0).max(4.0)),
                ),
                2,
                self.accent(),
            );
        }
        if deleted{response.context_menu(|ui|{if ui.button("Copy deleted text").clicked(){ui.ctx().copy_text(message.content.clone());ui.close();}});}else{response.context_menu(|ui|self.message_menu(ui,message));self.message_click(&response,message);self.quick_message_actions(ui,&response,message);}
    }
    /// Discord-style editing in place: an outlined box where the message text was, with
    /// "escape to cancel • enter to save" under it.
    fn inline_editor(&mut self, ui: &mut egui::Ui, message: &Message) {
        let accent=self.accent();
        let Some((_, text)) = self.edit.as_mut() else { return };
        let response=egui::Frame::NONE.fill(CARD).stroke(Stroke::new(1.5_f32,accent)).corner_radius(8).inner_margin(egui::Margin::symmetric(12,8)).show(ui,|ui|{
            ui.add(egui::TextEdit::multiline(text)
                .id_salt(("inline-edit",&message.id))
                .return_key(egui::KeyboardShortcut::new(egui::Modifiers::SHIFT,egui::Key::Enter))
                .desired_rows(1).desired_width(f32::INFINITY).frame(false))
        }).inner;
        if self.focus_edit{response.request_focus();self.focus_edit=false;}
        let (mut save,mut cancel)=(false,false);
        // The text box lets go of focus on Esc, so a box that just lost focus still counts.
        if response.has_focus()||response.lost_focus(){
            ui.input(|i|{
                if i.key_pressed(egui::Key::Escape){cancel=true;}
                if !i.events.iter().any(|e|matches!(e,egui::Event::Ime(_)))&&i.events.iter().any(|e|matches!(e,egui::Event::Key{key:egui::Key::Enter,pressed:true,repeat:false,modifiers,..}if !modifiers.shift)){save=true;}
            });
        }
        ui.horizontal(|ui|{
            ui.spacing_mut().item_spacing.x=0.0;
            let link=|ui:&mut egui::Ui,label:&str|ui.add(egui::Label::new(RichText::new(label).size(12.0).color(crate::message_media::LINK)).sense(egui::Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
            ui.label(RichText::new("escape to ").size(12.0).color(MUTED));
            if link(ui,"cancel"){cancel=true;}
            ui.label(RichText::new(" • enter to ").size(12.0).color(MUTED));
            if link(ui,"save"){save=true;}
        });
        if cancel{self.edit=None;}else if save{self.save_edit(&message.channel_id);}
    }
    /// Saves the message being edited in place.
    fn save_edit(&mut self, channel: &str) {
        let Some((id, content)) = self.edit.clone() else { return };
        let limit=if self.user.as_ref().is_some_and(|u|u.premium_type==2){4000}else{2000};
        let content = content.trim_end().to_owned();
        if content.trim().is_empty() { self.error = Some("A message can't be empty. Use Delete to remove it.".into()); return; }
        if content.chars().count() > limit { self.error = Some(format!("Message limit for this account: {limit} characters.")); return; }
        if self.preview {
            if let Some(message) = self.messages.iter_mut().find(|m| m.id == id) {
                message.content = content;
                message.edited_timestamp = Some("edited".into());
            }
        } else {
            self.send_command(Command::Edit { channel: channel.to_owned(), id, content });
        }
        self.edit = None;
    }
    fn react(&mut self, message: &Message, emoji: String, remove: bool) {
        if self.preview {
            if let Some(message) = self.messages.iter_mut().find(|m| m.id == message.id) {
                if let Some(reaction) = message
                    .reactions
                    .iter_mut()
                    .find(|r| r.emoji.route() == emoji)
                {
                    if remove {
                        reaction.count = reaction.count.saturating_sub(1);
                        reaction.me = false;
                    } else {
                        reaction.count += 1;
                        reaction.me = true;
                    }
                } else {
                    message.reactions.push(Reaction {
                        count: 1,
                        me: true,
                        emoji: Emoji {
                            id: None,
                            name: Some(emoji),
                        },
                    });
                }
                message.reactions.retain(|r| r.count > 0);
            }
        } else {
            self.send_command(Command::Reaction {
                channel: message.channel_id.clone(),
                id: message.id.clone(),
                emoji,
                remove,
            });
        }
    }
    /// Ctrl+V with copied files or an image attaches them like Discord; copied text is pasted by egui.
    fn paste_attachment(&mut self,channel:&str){
        match crate::clipboard::read(){
            Ok(Some(_)) if self.preview=>self.error=Some("Attachments are available after connecting to Discord.".into()),
            Ok(Some(crate::clipboard::Pasted::Files(files)))=>{if files.len()>1{self.error=Some(format!("Eclipse uploads one file per message; attached {}.",files[0].file_name().unwrap_or_default().to_string_lossy()));}self.files.insert(channel.into(),files[0].clone());},
            Ok(Some(crate::clipboard::Pasted::Image(path)))=>{self.files.insert(channel.into(),path);},
            Ok(None)=>{},
            Err(error)=>self.error=Some(error),
        }
    }
    fn composer(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, channel: &Channel) {
        if let Some(reply)=self.reply.clone().filter(|m|m.channel_id==channel.id){ui.horizontal(|ui|{ui.label(format!("Replying to {}: {}",reply.author.name(),crate::message_media::visible_content(&reply,self.prefs.images).chars().take(60).collect::<String>()));if ui.small_button("×").clicked(){self.reply=None;}});}
        if let Some(file) = self.files.get(&channel.id).cloned() {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "Attachment: {}",
                        file.file_name().unwrap_or_default().to_string_lossy()
                    ))
                    .size(12.0)
                    .color(self.accent()),
                );
                if ui.small_button("Remove").clicked() {
                    self.files.remove(&channel.id);
                }
            });
        }
        let mut send = false;
        let pending = self
            .pending
            .as_ref()
            .is_some_and(|(id, _)| id == &channel.id);
        egui::Frame::NONE
            .fill(CARD)
            .corner_radius(24)
            .inner_margin(egui::Margin::symmetric(12, 9))
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    if ui.add_enabled_ui(!pending,|ui|crate::widgets::control(ui,crate::widgets::Control::Upload,false,30.,"Attach a file")).inner.clicked(){
                        if self.preview{self.error=Some("Attachments are available after connecting to Discord.".into());}
                        else if let Some(path)=rfd::FileDialog::new().pick_file(){self.files.insert(channel.id.clone(),path);}
                    }
                    let mention_keys = self.mention_keys(ctx);
                    let draft = self.drafts.entry(channel.id.clone()).or_default();
                    let edit = egui::TextEdit::multiline(draft)
                        .id_salt(("composer",&channel.id))
                        .return_key(egui::KeyboardShortcut::new(egui::Modifiers::SHIFT,egui::Key::Enter))
                        .hint_text(format!("Message #{}", channel.label()))
                        .desired_rows(2)
                        .desired_width((ui.available_width() - 154.0).max(60.0))
                        .frame(false);
                    let response = ui.add_enabled(!pending, edit);
                    if self.focus_message_box{response.request_focus();self.focus_message_box=false;}
                    self.mention_popup(ctx, channel, &response, mention_keys);
                    let paste=response.has_focus()&&ctx.input(|i|i.focused)&&crate::clipboard::paste_keys_down();
                    if paste&&!self.paste_down{self.paste_attachment(&channel.id);}
                    self.paste_down=paste;
                    if response.has_focus(){ctx.input(|i|{for event in &i.events{match event{egui::Event::Ime(egui::ImeEvent::Enabled|egui::ImeEvent::Preedit(_))=>self.composer_ime=true,egui::Event::Ime(egui::ImeEvent::Disabled|egui::ImeEvent::Commit(_))=>self.composer_ime=false,_=>{}}}});}else{self.composer_ime=false;}
                    if response.changed()&&!self.prefs.silent_typing&&!self.preview&&self.last_typing.is_none_or(|t|t.elapsed()>Duration::from_secs(8)){self.last_typing=Some(Instant::now());self.mutate("typing",reqwest::Method::POST,format!("/channels/{}/typing",channel.id),None);}
                    if response.has_focus()
                        && !pending && !self.composer_ime && ctx.input(|i| i.events.iter().any(|e|matches!(e,egui::Event::Key{key:egui::Key::Enter,pressed:true,repeat:false,modifiers,..}if !modifiers.shift)) && !i.events.iter().any(|e|matches!(e,egui::Event::Ime(_))))
                    {
                        send = true;
                    }
                    for (mode,kind,tooltip) in [(crate::media_picker::Mode::Gif,crate::widgets::Control::Gif,"Choose a GIF"),(crate::media_picker::Mode::Emoji,crate::widgets::Control::Emoji,"Choose an emoji")]{
                        let response=crate::widgets::control(ui,kind,self.picker.mode==Some(mode),30.,tooltip);
                        if self.picker.mode==Some(mode){self.picker.anchor=Some(response.rect);}
                        if response.clicked(){if let Some(command)=self.picker.open(mode,&channel.id,self.guild.as_deref(),response.rect){if !self.preview{self.send_command(command);}}self.picker.other_servers=self.nitro_emojis();}
                    }
                    if ui
                        .add_enabled(
                            !pending && self.pending.is_none(),
                            primary(if pending { "Sending…" } else { "Send" }),
                        )
                        .clicked()
                    {
                        send = true;
                    }
                });
            });
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(if self.prefs.character_count{format!("{} / {}",self.drafts.get(&channel.id).map(|d|d.chars().count()).unwrap_or(0),if self.user.as_ref().is_some_and(|u|u.premium_type==2){4000}else{2000})}else{"Right-click a message for actions".into()})
                        .size(10.0)
                        .color(MUTED),
                );
            });
        });
        if send {
            self.send_message(channel);
        }
    }
    fn send_message(&mut self, channel: &Channel) {
        if self.pending.is_some() {
            return;
        }
        let content = crate::message_media::apply_mentions(&self.drafts.get(&channel.id).cloned().unwrap_or_default(), &self.mention_ids);
        let file = self.files.get(&channel.id).cloned();
        if content.trim().is_empty() && file.is_none() {
            return;
        }
        let limit=if self.user.as_ref().is_some_and(|u|u.premium_type==2){4000}else{2000};
        if content.chars().count() > limit {
            self.error = Some(format!("Message limit for this account: {limit} characters."));
            return;
        }
        if self.preview {
            merge_message(
                &mut self.messages,
                Message {
                    id: format!(
                        "local-{}",
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_nanos()
                    ),
                    channel_id: channel.id.clone(),
                    author: self.user.clone().unwrap_or_default(),
                    content,
                    timestamp: "Now".into(),
                    ..Default::default()
                },
            );
            self.drafts.remove(&channel.id);self.reply=None;
        } else {
            let nonce = match self.nonces.get(&channel.id) {
                Some((old_content, old_file, nonce,old_reference))
                    if old_content == &content && old_file == &file&&old_reference==&self.reply.as_ref().map(|m|m.id.clone()) =>
                {
                    nonce.clone()
                }
                _ => SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis()
                    .to_string(),
            };
            self.nonces.insert(
                channel.id.clone(),
                (content.clone(), file.clone(), nonce.clone(),self.reply.as_ref().map(|m|m.id.clone())),
            );
            if self.send_command(Command::Send {
                channel: channel.id.clone(),
                content: content.clone(),
                file,
                nonce,
                reference:self.reply.as_ref().filter(|m|m.channel_id==channel.id).map(|m|m.id.clone()),
            }) {
                self.pending = Some((channel.id.clone(), content));
            }
        }
    }
    fn dialogs(&mut self, ctx: &egui::Context) {
        self.extra_dialogs(ctx);
        if let Some(id) = self.delete.clone() {
            let mut open = true;
            egui::Window::new("Delete this message?")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("This removes the message from the conversation.");
                    ui.add_space(10.0);
                    if ui
                        .button(
                            RichText::new("Delete message").color(Color32::from_rgb(255, 160, 151)),
                        )
                        .clicked()
                    {
                        if let Some(channel) = self.channel.as_ref().map(|c| c.id.clone()) {
                            self.delete_now(&channel, &id);
                        }
                        self.delete = None;
                    }
                });
            if !open {
                self.delete = None;
            }
        }
        self.pins_dropdown(ctx);
        self.dm_picker(ctx);
    }
}

impl eframe::App for Eclipse {
    fn raw_input_hook(&mut self,ctx:&egui::Context,input:&mut egui::RawInput){
        // Offline visual diagnostics exercise the same hover/Shift path as real input.
        if self.preview&&self.preview_gesture.is_some(){
            if let Some(point)=ctx.data_mut(|d|d.get_temp::<egui::Pos2>(egui::Id::new("preview-gesture-point"))){input.events.push(egui::Event::PointerMoved(point));}
            if self.preview_gesture==Some("message"){input.modifiers.shift=true;}
        }
    }
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        BG.to_normalized_gamma_f32()
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_events();
        crate::zoom::handle(ctx,&mut self.prefs.zoom);
        crate::zoom::accelerate_scroll(ctx);
        self.preferences_tick(ctx);
        self.calls.poll();
        if !self.calls.active(){self.voice_revealed=None;}
        self.images.poll(ctx);
        if self.font_scan_needed&&!self.extra_emoji_font {
            self.font_scan_needed=false;
            let names:Vec<_>=self.channels.iter().map(Channel::label).chain(self.dms.iter().map(Channel::label)).collect();
            if crate::widgets::needs_emoji_font(ctx,names.iter().map(String::as_str).chain(self.messages.iter().map(|m|m.content.as_str()))) {
                self.extra_emoji_font=true;crate::widgets::fonts_with_emoji(ctx,true);ctx.request_repaint();
            }
        }
        if self.last_sample.elapsed() > Duration::from_secs(2) {
            self.memory = crate::memory();
            self.last_sample = Instant::now();
        }
        ctx.request_repaint_after(Duration::from_secs(self.prefs.idle_seconds));
        if ctx.input(|i|i.modifiers.ctrl&&i.key_pressed(egui::Key::Comma)){self.open_settings();}
        if ctx.input(|i|i.modifiers.ctrl&&i.modifiers.shift&&i.key_pressed(egui::Key::M)){self.calls.toggle_mute();}
        if ctx.input(|i|i.modifiers.ctrl&&i.modifiers.shift&&i.key_pressed(egui::Key::D)){self.calls.toggle_deafen();}
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::R)) {
            if let Some(channel) = &self.channel {
                if !self.preview {
                    self.send_command(Command::History(channel.id.clone(), None));
                }
            }
        }
        // Full-screen stream: nothing but the call stage.
        let full = self.user.is_some() && self.calls.fullscreen();
        if !full { egui::TopBottomPanel::bottom("status")
            .exact_height(if self.compact { 22.0 } else { 28.0 })
            .frame(
                egui::Frame::NONE
                    .fill(RAIL)
                    .inner_margin(egui::Margin::symmetric(if self.compact { 10 } else { 16 }, if self.compact { 3 } else { 5 })),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("ECLIPSE  /  NATIVE RUST")
                            .size(10.0)
                            .color(MUTED),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!(
                                "{:.0} MB RAM  ·  1 process",
                                self.memory.working_mb
                            ))
                            .size(11.0)
                            .color(
                                if self.memory.working_mb > 300.0 {
                                    Color32::from_rgb(242, 198, 119)
                                } else {
                                    self.accent()
                                },
                            ),
                        );
                        ui.label(RichText::new(&self.status).size(11.0).color(MUTED));
                    });
                });
            }); }
        if self.user.is_none() {
            self.login(ctx);
        } else if full {
            self.sync_call_roster();
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::BLACK)).show(ctx,|ui|self.calls.stage(ui,&mut self.images));
        } else {
            self.left_column(ctx);
            if self.home!=Home::Chat{self.home_panel(ctx);}else{self.conversation_header(ctx);if self.calls.active(){self.sync_call_roster();}
                if let Some(moved)=self.calls.moved_channel().map(str::to_owned){if let Some(channel)=self.channels.iter().find(|c|c.id==moved).cloned(){self.calls.set_moved_channel(channel);}}
                if self.calls.active()&&!self.calls.chat{egui::CentralPanel::default().frame(egui::Frame::NONE.fill(preferences_bg(&self.prefs)).inner_margin(if self.compact{0}else{8})).show(ctx,|ui|self.calls.stage(ui,&mut self.images));}else{self.members(ctx);self.conversation(ctx);}}
        }
        self.dialogs(ctx);
        if self.home!=Home::Chat||self.settings||self.server_settings||self.channel.as_ref().is_none_or(|c|c.id!=self.picker.channel)||self.calls.active()&&!self.calls.chat{self.picker.close();}
        let (picked, command) = self.picker.show(ctx, &mut self.images);
        if let Some((channel, text)) = picked {
            let draft = self.drafts.entry(channel).or_default();
            if !draft.is_empty() {
                draft.push(' ');
            }
            draft.push_str(&text);
        }
        if let Some(command) = command {
            if self.preview {
                self.picker.pending = false;
                self.error = Some("Connect to Discord to search GIFs.".into());
            } else {
                self.send_command(command);
            }
        }
        for command in self.calls.show(ctx, &mut self.images) {
            self.send_gateway(command);
        }
        // Remember Screen Share dialog choices, and send your stream's preview picture.
        let (height, fps, preview) = self.calls.share_choices();
        if (self.prefs.screen_height, self.prefs.screen_fps, self.prefs.stream_preview) != (height, fps, preview) {
            (self.prefs.screen_height, self.prefs.screen_fps, self.prefs.stream_preview) = (height, fps, preview);
        }
        if let Some((key, thumbnail)) = self.calls.take_preview_upload() {
            self.mutate("stream-preview-upload", reqwest::Method::POST, format!("/streams/{key}/preview"), Some(serde_json::json!({"thumbnail": thumbnail})));
        }
        if let Some(channel) = self.incoming_call.clone() {
            egui::Window::new("Incoming Discord call")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(channel.label());
                    ui.horizontal(|ui| {
                        for (label, video) in [("Answer", false), ("Answer with video", true)] {
                            if ui.button(label).clicked() {
                                if let Some(user) = &self.user {
                                    match self.calls.join(&channel, user, video) {
                                        Ok(payload) => self.send_gateway(payload),
                                        Err(error) => self.error = Some(error.into()),
                                    }
                                }
                                self.incoming_call = None;
                            }
                        }
                        if ui.button("Dismiss").clicked() {
                            self.incoming_call = None;
                        }
                    });
                });
        }
        if let Some(path) = &self.smoke {
            if self.started.elapsed() > Duration::from_secs(4) && !self.screenshot_requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                self.screenshot_requested = true;
            }
            for event in ctx.input(|i| i.events.clone()) {
                if let egui::Event::Screenshot { image, .. } = event {
                    let pixels: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                    if image::save_buffer(
                        path,
                        &pixels,
                        image.width() as u32,
                        image.height() as u32,
                        image::ColorType::Rgba8,
                    )
                    .is_ok()
                    {
                        self.screenshot_saved = true;
                    }
                    let stats = crate::memory();
                    let report = serde_json::json!({"working_set_mb":stats.working_mb,"private_commit_mb":stats.private_mb,"peak_working_set_mb":stats.peak_mb,"mode":if self.preview{"offline_preview"}else{"sign_in"},"messages":self.messages.len(),"cached_icons":self.images.len(),"animated_icons":self.images.animated(),"animation_frame_uploads":self.images.uploads(),"decoded_icon_bytes":self.images.bytes(),"expanded_folders":self.open_folders.len(),"screenshot_saved":self.screenshot_saved});
                    let _ = std::fs::write(
                        format!("{path}.json"),
                        serde_json::to_vec_pretty(&report).unwrap(),
                    );
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            if self.started.elapsed() > Duration::from_secs(15) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

impl Drop for Eclipse {
    fn drop(&mut self) {
        if self.prefs_save_at.is_some() || self.prefs!=self.applied_prefs {
            self.prefs.normalize();let _=self.prefs.save();
        }
    }
}

fn primary(text: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).color(SIDE).strong())
        .fill(ACCENT)
        .corner_radius(18)
}
fn surface() -> egui::Frame {
    egui::Frame::NONE
        .fill(SIDE)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(18)
}
fn name_color(name: &str) -> Color32 {
    let colors = [
        Color32::from_rgb(205, 164, 238),
        Color32::from_rgb(227, 211, 122),
        Color32::from_rgb(116, 199, 184),
        Color32::from_rgb(233, 133, 145),
        Color32::from_rgb(140, 164, 221),
        Color32::from_rgb(217, 154, 205),
    ];
    let index = name.bytes().fold(0usize, |a, b| a.wrapping_add(b as usize)) % colors.len();
    colors[index]
}
fn moon_button(ui: &mut egui::Ui,selected:bool) -> egui::Response {
    let (rect,response)=ui.allocate_exact_size(Vec2::splat(44.0),egui::Sense::click());
    let fill=if selected{Color32::from_gray(if response.hovered(){202}else{184})}else if response.hovered(){Color32::from_gray(49)}else{CARD};
    let p = ui.painter();
    p.rect_filled(rect,13,fill);
    let c = response.rect.center();
    p.circle_filled(c, 12.0, if selected{Color32::BLACK}else{MUTED});
    p.circle_filled(c + Vec2::new(6.0, -4.0), 11.0, fill);
    response
}
fn initials_of(name: &str) -> String {
    let mut words = name.split_whitespace();
    let first = words.next().and_then(|s| s.chars().next()).unwrap_or('?');
    let second = words.next().and_then(|s| s.chars().next());
    match second {
        Some(s) => format!("{first}{s}"),
        None => first.to_uppercase().collect(),
    }
}
fn server_button(ui: &mut egui::Ui, text: &str, selected: bool) -> egui::Response {
    ui.add_sized(
        [44.0, 44.0],
        egui::Button::new(RichText::new(text).strong().size(15.0).color(if selected {
            SIDE
        } else {
            MUTED
        }))
        .fill(if selected { ACCENT } else { CARD })
        .corner_radius(if selected { 15 } else { 24 }),
    )
}
fn eclipse_mark(ui:&mut egui::Ui,size:f32) {
    let id=egui::Id::new("eclipse-brand-icon");
    let texture=ui.ctx().data_mut(|d|d.get_temp::<egui::TextureHandle>(id));
    let texture=texture.unwrap_or_else(||{
        let icon=crate::icon();
        let pixels=egui::ColorImage::from_rgba_unmultiplied([icon.width as usize,icon.height as usize],&icon.rgba);
        let texture=ui.ctx().load_texture("Eclipse logo",pixels,egui::TextureOptions::LINEAR);
        ui.ctx().data_mut(|d|d.insert_temp(id,texture.clone()));texture
    });
    let (rect,_)=ui.allocate_exact_size(Vec2::splat(size),egui::Sense::hover());
    egui::Image::new((texture.id(),rect.size())).paint_at(ui,rect);
}
fn avatar(ui: &mut egui::Ui, name: &str, size: f32) -> egui::Rect {
    avatar_response(ui,name,size,egui::Sense::hover()).0
}
fn avatar_response(ui:&mut egui::Ui,name:&str,size:f32,sense:egui::Sense)->(egui::Rect,egui::Response){
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), sense);
    let palette = [
        Color32::from_rgb(64, 82, 100),
        Color32::from_rgb(85, 69, 101),
        Color32::from_rgb(57, 89, 80),
        Color32::from_rgb(102, 78, 62),
    ];
    let index = name.bytes().fold(0usize, |a, b| a.wrapping_add(b as usize)) % palette.len();
    ui.painter()
        .circle_filled(rect.center(), size / 2.0, palette[index]);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        initials_of(name),
        egui::FontId::proportional(size * 0.37),
        TEXT,
    );
    (rect,response)
}

fn preferences_bg(p:&crate::preferences::Preferences)->Color32{crate::preferences::color(&p.theme.background).unwrap_or(BG)}

#[cfg(test)]
mod interaction_tests {
    use super::*;
    #[test]fn dm_activity_moves_conversations_up_without_regressing_on_older_messages(){
        let ctx=egui::Context::default();let mut app=Eclipse::with_context(&ctx,true,None);
        app.dms=vec![Channel{id:"newest".into(),last_message_id:Some("100".into()),kind:1,..Default::default()},Channel{id:"older".into(),last_message_id:Some("9".into()),kind:1,..Default::default()}];
        app.guild=None;app.channels=app.dms.clone();
        app.receive_message(Message{id:"101".into(),channel_id:"older".into(),..Default::default()},true);
        assert_eq!(app.dms[0].id,"older");assert_eq!(app.channels[0].id,"older");assert_eq!(app.dms[0].last_message_id.as_deref(),Some("101"));
        app.receive_message(Message{id:"8".into(),channel_id:"older".into(),..Default::default()},true);
        assert_eq!(app.dms[0].last_message_id.as_deref(),Some("101"));
        app.guild=Some("server".into());app.channels=vec![Channel{id:"server-channel".into(),..Default::default()}];
        app.receive_message(Message{id:"102".into(),channel_id:"newest".into(),..Default::default()},false);
        assert_eq!(app.dms[0].id,"newest");assert_eq!(app.channels[0].id,"server-channel");
    }
    #[test]fn dm_list_starts_with_25_and_reveals_more_only_when_scrolled_near_the_end(){
        let ctx=egui::Context::default();let mut app=Eclipse::with_context(&ctx,true,None);
        app.guild=None;app.channel=None;
        app.dms=(0..60).map(|i|Channel{id:format!("dm-{i}"),name:Some(format!("Conversation {i:02}")),kind:1,last_message_id:Some((100+i).to_string()),..Default::default()}).collect();
        crate::navigation::sort_dms(&mut app.dms);app.channels=app.dms.clone();
        for _ in 0..3{let _=ctx.run(input(vec![]),|ctx|app.left_column(ctx));}
        assert_eq!(app.dm_visible,25);
        let point=egui::pos2(200.,430.);
        for _ in 0..40{
            let _=ctx.run(input(vec![egui::Event::PointerMoved(point),egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Line,delta:Vec2::new(0.,-12.),modifiers:Default::default()}]),|ctx|app.left_column(ctx));
            if app.dm_visible>25{break;}
        }
        assert_eq!(app.dm_visible,50,"scrolling to the end reveals the next batch");
        // Searching filters all conversations before applying the first-page limit.
        ctx.memory_mut(|m|m.request_focus(egui::Id::new("dm-conversation-search")));
        let output=ctx.run(input(vec![egui::Event::Text("Conversation 00".into())]),|ctx|app.left_column(ctx));
        assert_eq!(app.dm_visible,25);
        assert!(output.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.job.text=="Conversation 00")));
    }
    #[test]fn consecutive_messages_share_author_heading_and_keep_all_bodies_aligned(){
        let ctx=egui::Context::default();let mut app=Eclipse::with_context(&ctx,true,None);let channel=app.channel.clone().unwrap();
        let message=|id:&str,author:&str,content:&str|Message{id:id.into(),channel_id:channel.id.clone(),author:User{id:author.into(),username:author.into(),..Default::default()},content:content.into(),timestamp:"2026-10-09T12:00:00Z".into(),..Default::default()};
        app.messages=VecDeque::from([message("1","Repeated author","First in group"),message("2","Repeated author","Second in group"),message("3","Other author","Other message"),message("4","Repeated author","New group")]);
        let mut output=None;for _ in 0..3{output=Some(ctx.run(input(vec![]),|ctx|app.conversation(ctx)));}
        let output=output.unwrap();let texts:Vec<_>=output.shapes.iter().filter_map(|s|if let egui::Shape::Text(t)=&s.shape{Some(t)}else{None}).collect();
        assert_eq!(texts.iter().filter(|t|t.galley.job.text=="Repeated author").count(),2);
        assert_eq!(texts.iter().filter(|t|t.galley.job.text=="Other author").count(),1);
        // Glyph bearings differ (e.g. F versus S); compare layout origins, not ink bounds.
        let body=|text:&str|texts.iter().find(|t|t.galley.job.text==text).expect("message body").pos;
        let first=body("First in group");let second=body("Second in group");
        assert!((first.x-second.x).abs()<1.0,"message origins differ: {first:?}, {second:?}");
        body("Other message");body("New group");
        assert!(!texts.iter().any(|t|t.galley.job.text.contains("Enter to send")));
    }
    #[test]fn server_channels_start_below_the_header_without_search_or_redundant_label(){
        let ctx=egui::Context::default();let mut app=Eclipse::with_context(&ctx,true,None);let mut output=None;
        for _ in 0..2{output=Some(ctx.run(input(vec![]),|ctx|{app.left_column(ctx);}));}
        let output=output.unwrap();let labels:Vec<_>=output.shapes.iter().filter_map(|s|if let egui::Shape::Text(t)=&s.shape{Some(t)}else{None}).collect();
        assert!(!labels.iter().any(|t|matches!(t.galley.job.text.as_str(),"Find a channel"|"Find a conversation"|"Conversations")));
        let category=labels.iter().find(|t|t.galley.job.text=="Community").expect("first channel category");let header=ctx.read_response(egui::Id::new("server-header-click")).unwrap().rect;
        assert!(category.visual_bounding_rect().top()-header.bottom()<60.0,"channel list must move up");
        app.guild=None;app.channels=app.dms.clone();
        let output=ctx.run(input(vec![]),|ctx|{app.left_column(ctx);});let texts:Vec<_>=output.shapes.iter().filter_map(|s|if let egui::Shape::Text(t)=&s.shape{Some(t.galley.job.text.clone())}else{None}).collect();
        // The search bar sits at the top and the old separate box under Quests is gone.
        assert_eq!(texts.iter().filter(|t|t.as_str()=="Find A Conversation").count(),1,"{texts:?}");assert!(!texts.iter().any(|t|t=="Find a conversation"));
    }
    #[test]fn member_panel_has_no_count_or_search_and_header_search_names_the_server(){
        let ctx=egui::Context::default();let mut app=Eclipse::with_context(&ctx,true,None);app.show_members=true;app.prefs.member_count=true;
        app.server.select(app.guild.as_deref().unwrap());let guild=app.server.id.clone();
        app.server.ingest("GUILD_MEMBER_LIST_UPDATE",&serde_json::json!({"guild_id":"another-server","online_count":999}));assert!(app.server.online_count.is_none());
        app.server.ingest("GUILD_MEMBER_LIST_UPDATE",&serde_json::json!({"guild_id":guild,"online_count":42}));assert_eq!(app.server.online_count,Some(42));
        let texts=|output:&egui::FullOutput|output.shapes.iter().filter_map(|s|if let egui::Shape::Text(t)=&s.shape{Some(t.galley.job.text.clone())}else{None}).collect::<Vec<_>>();
        let mut output=None;for _ in 0..2{output=Some(ctx.run(input(vec![]),|ctx|{app.conversation_header(ctx);app.members(ctx);}));}
        let labels=texts(output.as_ref().unwrap());let server=app.guilds.iter().find(|g|Some(&g.id)==app.guild.as_ref()).unwrap().name.clone();
        assert!(!labels.iter().any(|t|t.ends_with(" online")||t=="Search members"||t=="Pins"||t=="People"||t=="↻"),"{labels:?}");
        assert!(labels.contains(&format!("Search {server}")),"{labels:?}");
        let shown=|app:&mut Eclipse|texts(&ctx.run(input(vec![]),|ctx|app.members(ctx))).len();
        let before=shown(&mut app);app.search="zz-no-such-member".into();assert!(shown(&mut app)<before,"header search filters the member list");
        app.search.clear();app.guild=None;
        let output=ctx.run(input(vec![]),|ctx|app.conversation_header(ctx));assert!(texts(&output).iter().any(|t|t=="Search this conversation"));
    }
    #[test]fn clicking_the_same_user_again_closes_their_profile(){
        let ctx=egui::Context::default();let mut app=Eclipse::with_context(&ctx,true,None);let user=app.messages.front().unwrap().author.clone();let anchor=egui::Rect::from_min_size(egui::pos2(300.,200.),Vec2::splat(30.));
        app.toggle_profile_at(&user,anchor);app.toggle_profile_at(&user,anchor);assert!(app.profile.is_some(),"nested hit regions in one frame keep it open");
        let _=ctx.run(input(vec![]),|ctx|{app.profile_popout(ctx);});
        app.toggle_profile_at(&user,anchor);assert!(app.profile.is_none());
        app.toggle_profile_at(&user,anchor);assert!(app.profile.is_some());
    }
    #[test]fn shift_message_controls_edit_and_delete_only_when_permitted(){
        for grouped in [false,true] {
        let ctx=egui::Context::default();let mut app=Eclipse::with_context(&ctx,true,None);
        let mut message=app.messages.front().unwrap().clone();message.author=app.user.clone().unwrap();
        let id=egui::Id::new(("message-actions",&message.channel_id,&message.id));
        let frame=|app:&mut Eclipse,message:&Message,shift,events|{let mut raw=input(events);raw.modifiers.shift=shift;ctx.run(raw,|ctx|{egui::CentralPanel::default().show(ctx,|ui|app.message_ui_grouped(ui,message,grouped));})};
        for _ in 0..3{frame(&mut app,&message,true,vec![egui::Event::PointerMoved(egui::pos2(700.0,24.0))]);}
        let edit=ctx.read_response(id.with("edit")).expect("Shift edit control").rect.center();
        for pressed in [true,false]{frame(&mut app,&message,true,vec![egui::Event::PointerMoved(edit),egui::Event::PointerButton{pos:edit,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::SHIFT}]);}
        assert_eq!(app.edit,Some((message.id.clone(),message.content.clone())));
        let delete=ctx.read_response(id.with("delete")).unwrap().rect.center();let before=app.messages.len();
        for pressed in [true,false]{frame(&mut app,&message,true,vec![egui::Event::PointerMoved(delete),egui::Event::PointerButton{pos:delete,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::SHIFT}]);}
        assert_eq!(app.delete,None,"quick delete must not ask for confirmation");assert_eq!(app.messages.len(),before-1,"quick delete removes the message at once");
        for _ in 0..2{frame(&mut app,&message,false,vec![]);}assert!(ctx.read_response(id.with("edit")).is_none());
        message.author.id="someone-else".into();app.guild=None;
        frame(&mut app,&message,true,vec![egui::Event::PointerMoved(egui::pos2(700.0,24.0))]);assert!(ctx.read_response(id.with("edit")).is_none()&&ctx.read_response(id.with("delete")).is_none());
        }
    }
    #[test]fn editing_opens_a_focused_box_in_the_message_and_enter_saves_escape_cancels(){
        let ctx=egui::Context::default();let mut app=Eclipse::with_context(&ctx,true,None);
        let mut message=app.messages.back().unwrap().clone();message.author=app.user.clone().unwrap();
        if let Some(m)=app.messages.back_mut(){m.author=message.author.clone();}
        let frame=|app:&mut Eclipse,message:&Message,events|{ctx.run(input(events),|ctx|{egui::CentralPanel::default().show(ctx,|ui|app.message_ui(ui,message));})};
        app.start_edit(&message);frame(&mut app,&message,vec![]);
        let focused=ctx.memory(|m|m.focused());assert!(focused.is_some(),"the edit box takes focus straight away");
        frame(&mut app,&message,vec![egui::Event::Text(" (edited)".into())]);
        frame(&mut app,&message,vec![egui::Event::Key{key:egui::Key::Enter,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
        assert!(app.edit.is_none());let saved=app.messages.iter().find(|m|m.id==message.id).unwrap();
        assert_eq!(saved.content,format!("{} (edited)",message.content));
        app.start_edit(&message);frame(&mut app,&message,vec![]);
        frame(&mut app,&message,vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
        assert!(app.edit.is_none());
    }
    #[test]fn voice_moderation_menu_sends_mute_and_disconnect_and_is_hidden_without_permission(){
        let ctx=egui::Context::default();
        let state=crate::voice_roster::VoiceState{user_id:"77".into(),server_muted:true,..Default::default()};
        let destinations=vec![("9".to_owned(),"Lounge".to_owned())];
        let run=|events:Vec<egui::Event>,allowed:bool|{let mut chosen=None;let output=ctx.run(input(events),|ctx|{egui::CentralPanel::default().show(ctx,|ui|{chosen=panels::voice_moderation_menu(ui,&state,allowed,allowed,allowed,&destinations);});});(chosen,output)};
        let (_,output)=run(vec![],true);
        let at=|label:&str|output.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.job.text==label=>Some(t.visual_bounding_rect().center()),_=>None}).unwrap_or_else(||panic!("{label} shown"));
        let (mute,disconnect)=(at("Server Mute"),at("Disconnect"));
        let click=|pos|vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()},egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}];
        assert_eq!(run(click(disconnect),true).0,Some(("77".to_owned(),serde_json::json!({"channel_id":null}))));
        assert_eq!(run(click(mute),true).0,Some(("77".to_owned(),serde_json::json!({"mute":false}))),"server-muted, so the click unmutes");
        let (chosen,output)=run(vec![],false);
        assert!(chosen.is_none()&&!output.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.job.text=="Disconnect")),"no moderation without permission");
    }
    #[test]fn channels_created_or_deleted_by_a_bot_show_up_live(){
        let ctx=egui::Context::default();let mut app=Eclipse::with_context(&ctx,true,None);
        let guild=app.guild.clone().expect("sample server");
        app.account_event("CHANNEL_CREATE",&serde_json::json!({"id":"9001","guild_id":guild,"type":2,"name":"Jason's channel","position":99}));
        assert!(app.channels.iter().any(|c|c.id=="9001"&&c.kind==2),"a new voice channel appears without switching servers");
        app.account_event("CHANNEL_DELETE",&serde_json::json!({"id":"9001","guild_id":guild,"type":2}));
        assert!(!app.channels.iter().any(|c|c.id=="9001"));
        app.account_event("CHANNEL_CREATE",&serde_json::json!({"id":"9002","guild_id":"another-server","type":2,"name":"elsewhere"}));
        assert!(!app.channels.iter().any(|c|c.id=="9002"),"other servers' channels stay out of this list");
    }
    #[test]fn nitro_offers_every_servers_emojis_and_others_only_get_the_current_server(){
        let ctx=egui::Context::default();let mut app=Eclipse::with_context(&ctx,true,None);
        let current=app.guild.clone().expect("sample server");
        app.account_event("GUILD_CREATE",&serde_json::json!({"id":current,"name":"Eclipse Lab","emojis":[{"id":"1","name":"here"}]}));
        app.account_event("GUILD_CREATE",&serde_json::json!({"id":"700","name":"Gaming","emojis":[{"id":"2","name":"pog","animated":true}]}));
        app.account_event("GUILD_EMOJIS_UPDATE",&serde_json::json!({"guild_id":"700","emojis":[{"id":"2","name":"pog","animated":true},{"id":"3","name":"gg"}]}));
        if let Some(user)=app.user.as_mut(){user.premium_type=0;}
        assert!(app.nitro_emojis().is_empty(),"without Nitro only the current server's emojis are offered");
        if let Some(user)=app.user.as_mut(){user.premium_type=2;}
        let servers=app.nitro_emojis();
        assert_eq!(servers.len(),1,"the current server is not repeated");
        assert_eq!(servers[0].0,"Gaming");assert_eq!(servers[0].1.iter().map(|e|e.token()).collect::<Vec<_>>(),["<a:pog:2>","<:gg:3>"]);
        app.guild=None;assert_eq!(app.nitro_emojis().len(),2,"in DMs every server's emojis are offered");
    }
    fn composer_frame(ctx:&egui::Context,app:&mut Eclipse,channel:&Channel,events:Vec<egui::Event>)->egui::FullOutput{ctx.run(input(events),|ctx|{egui::CentralPanel::default().show(ctx,|ui|{app.composer(ui,ctx,channel);});})}
    #[test]fn typing_at_lists_members_and_enter_inserts_a_mention_sent_as_an_id(){
        let ctx=egui::Context::default();let mut app=Eclipse::with_context(&ctx,true,None);let channel=app.channel.clone().unwrap();let before=app.messages.len();
        // A member with a real-looking numeric id, sorting first for "jo".
        let mut member=app.server.members.values().next().expect("sample member").clone();member.user.id="4242".into();member.user.username="joanne".into();member.nick=Some("Joanne".into());
        let jordan=member.user.clone();app.server.members.insert("4242".into(),member);
        let enter=egui::Event::Key{key:egui::Key::Enter,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()};
        let out=composer_frame(&ctx,&mut app,&channel,vec![]);
        let point=out.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.job.text.starts_with("Message #")=>Some(t.visual_bounding_rect().center()),_=>None}).expect("composer hint");
        for pressed in [true,false]{composer_frame(&ctx,&mut app,&channel,vec![egui::Event::PointerMoved(point),egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
        composer_frame(&ctx,&mut app,&channel,vec![egui::Event::Text("@".into())]);
        assert!(app.mention_open,"typing @ opens the list");
        composer_frame(&ctx,&mut app,&channel,vec![egui::Event::Text("jo".into())]);
        composer_frame(&ctx,&mut app,&channel,vec![enter.clone()]);
        assert_eq!(app.drafts[&channel.id],format!("@{} ",jordan.username),"Enter picks the highlighted member instead of sending");
        assert_eq!(app.messages.len(),before);
        composer_frame(&ctx,&mut app,&channel,vec![egui::Event::Key{key:egui::Key::Enter,physical_key:None,pressed:false,repeat:false,modifiers:Default::default()}]);
        composer_frame(&ctx,&mut app,&channel,vec![egui::Event::Text("hi".into()),enter]);
        let sent=app.messages.back().unwrap();assert_eq!(sent.content,format!("<@{}> hi",jordan.id));
        assert_eq!(app.readable(sent,&sent.content),"@Joanne hi");
        assert!(app.suggestions(&channel,"ev").iter().any(|s|s.label=="@everyone"));
    }
    #[test]fn composer_enter_sends_shift_enter_inserts_newline_and_unfocused_enter_does_nothing(){
        let ctx=egui::Context::default();let mut app=Eclipse::with_context(&ctx,true,None);let channel=app.channel.clone().unwrap();let before=app.messages.len();
        let enter=|shift|egui::Event::Key{key:egui::Key::Enter,physical_key:None,pressed:true,repeat:false,modifiers:egui::Modifiers{shift,..Default::default()}};
        app.drafts.insert(channel.id.clone(),"hello".into());composer_frame(&ctx,&mut app,&channel,vec![enter(false)]);assert_eq!(app.messages.len(),before);
        app.drafts.remove(&channel.id);let out=composer_frame(&ctx,&mut app,&channel,vec![]);
        let point=out.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.job.text.starts_with("Message #")=>Some(t.visual_bounding_rect().center()),_=>None}).expect("composer hint");
        for pressed in [true,false]{composer_frame(&ctx,&mut app,&channel,vec![egui::Event::PointerMoved(point),egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
        assert!(ctx.memory(|m|m.focused().is_some()));
        composer_frame(&ctx,&mut app,&channel,vec![egui::Event::Text("hello".into()),enter(true)]);assert_eq!(app.drafts[&channel.id],"hello\n");assert_eq!(app.messages.len(),before);
        composer_frame(&ctx,&mut app,&channel,vec![egui::Event::Key{key:egui::Key::Enter,physical_key:None,pressed:false,repeat:false,modifiers:Default::default()}]);
        composer_frame(&ctx,&mut app,&channel,vec![egui::Event::Text("second".into()),enter(false)]);assert_eq!(app.messages.back().unwrap().content,"hello\nsecond");assert!(!app.drafts.contains_key(&channel.id));
        let sent=app.messages.len();composer_frame(&ctx,&mut app,&channel,vec![enter(false)]);assert_eq!(app.messages.len(),sent);
    }
    #[test]fn both_settings_panels_close_on_outside_click_and_keep_inside_clicks(){
        for server in [false,true]{let ctx=egui::Context::default();let mut app=Eclipse::with_context(&ctx,true,None);app.settings=!server;app.server_settings=server;
            for _ in 0..3{let _=ctx.run(input(vec![]),|ctx|{if server{app.server_settings_window(ctx);}else{app.settings_window(ctx);}});}
            let rect=ctx.memory(|m|m.area_rect(egui::Id::new(if server{"server-settings"}else{"user-settings"}))).expect("settings panel");assert!(rect.left()>=0.&&rect.top()>=0.&&rect.right()<=1080.&&rect.bottom()<=680.,"{rect:?}");
            for(point,open)in[(egui::pos2(700.,200.),true),(egui::pos2(10.,10.),false)]{let events=vec![egui::Event::PointerMoved(point),egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()},egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}];let _=ctx.run(input(events),|ctx|{if server{app.server_settings_window(ctx);}else{app.settings_window(ctx);}});assert_eq!(if server{app.server_settings}else{app.settings},open);}
        }
    }
    #[test]fn server_icon_name_and_caret_open_the_same_anchored_menu(){
        for fraction in [0.05,0.45,0.95]{let ctx=egui::Context::default();let mut app=Eclipse::with_context(&ctx,true,None);
            let _=ctx.run(input(vec![]),|ctx|{app.left_column(ctx);});let rect=ctx.read_response(egui::Id::new("server-header-click")).unwrap().rect;let point=egui::pos2(rect.left()+rect.width()*fraction,rect.center().y);
            for pressed in [true,false]{let _=ctx.run(input(vec![egui::Event::PointerMoved(point),egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]),|ctx|{app.left_column(ctx);});}
            assert!(egui::Popup::is_id_open(&ctx,egui::Id::new("server-header-menu")),"header click at {fraction}");
        }
    }
    #[test]fn repeated_chat_author_avatars_each_open_their_own_anchor(){
        let ctx=egui::Context::default();let mut app=Eclipse::with_context(&ctx,true,None);
        let message=app.messages.front().unwrap().clone();let mut point=egui::Pos2::ZERO;
        for phase in 0..3{
            let mut events=vec![];if phase>0{events=vec![egui::Event::PointerMoved(point),egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed:phase==1,modifiers:Default::default()}];}
            let output=ctx.run(input(events),|ctx|{egui::CentralPanel::default().show(ctx,|ui|{app.message_ui(ui,&message);app.message_ui(ui,&message);});});
            if phase==0{point=output.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Circle(c)if c.radius==20.=>Some(c.center),_=>None}).expect("chat avatar missing");}
        }
        assert_eq!(app.profile.as_ref().map(|u|&u.id),Some(&message.author.id));assert!(app.profile_anchor.unwrap().contains(point));
    }
    #[test]fn dm_dropdown_is_anchored_clamped_and_dismisses_on_escape(){
        let ctx=egui::Context::default();let mut app=Eclipse::with_context(&ctx,true,None);
        app.open_dm_picker(egui::Rect::from_min_size(egui::pos2(975.,630.),Vec2::splat(28.)));
        let mut rect=egui::Rect::NOTHING;
        for _ in 0..3{let _=ctx.run(input(vec![]),|ctx|{egui::CentralPanel::default().show(ctx,|ui|{ui.label("DMs");});rect=app.dm_picker(ctx).unwrap();});}
        assert!(rect.left()>=0.&&rect.top()>=0.&&rect.right()<=1080.&&rect.bottom()<=680.,"{rect:?}");
        let _=ctx.run(input(vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]),|ctx|{app.dm_picker(ctx);});assert!(!app.dm_modal);
    }
    #[test]fn dm_context_menu_exposes_user_actions_without_a_recipient_submenu(){
        let ctx=egui::Context::default();let mut app=Eclipse::with_context(&ctx,true,None);let channel=app.dms[0].clone();
        let output=ctx.run(input(vec![]),|ctx|{egui::CentralPanel::default().show(ctx,|ui|{app.channel_menu(ui,&channel);});});
        let labels:Vec<_>=output.shapes.iter().filter_map(|s|if let egui::Shape::Text(t)=&s.shape{Some(t.galley.job.text.as_str())}else{None}).collect();
        assert!(labels.contains(&"Profile")&&labels.contains(&"Message")&&labels.contains(&"Block"));assert!(!labels.contains(&"Recipient"));
    }
    fn input(events:Vec<egui::Event>)->egui::RawInput {egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,Vec2::new(1080.,680.))),events,..Default::default()}}
    #[test]
    fn profile_popout_stays_on_screen_and_closes_on_outside_click_and_escape(){
        let ctx=egui::Context::default();let mut app=Eclipse::with_context(&ctx,true,None);
        let user=app.messages.front().unwrap().author.clone();
        app.show_profile_at(&user,egui::Rect::from_min_size(egui::pos2(980.,580.),Vec2::new(80.,40.)));
        let mut rect=egui::Rect::NOTHING;
        let mut visible_name=false;
        for _ in 0..3{let output=ctx.run(input(vec![]),|ctx|{egui::CentralPanel::default().show(ctx,|ui|{ui.label("Conversation");});rect=app.profile_popout(ctx).unwrap();});visible_name=output.shapes.iter().any(|shape|matches!(&shape.shape,egui::epaint::Shape::Text(text) if text.galley.job.text==user.name()&&shape.clip_rect.intersects(text.visual_bounding_rect())));}
        assert!(rect.left()>=0.&&rect.top()>=0.&&rect.right()<=1080.&&rect.bottom()<=680.,"{rect:?}");
        assert!(visible_name,"profile details must be visible below the header");
        let outside=egui::pos2(15.,600.);let events=vec![egui::Event::PointerMoved(outside),egui::Event::PointerButton{pos:outside,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()},egui::Event::PointerButton{pos:outside,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}];
        let _=ctx.run(input(events),|ctx|{egui::CentralPanel::default().show(ctx,|_|{});app.profile_popout(ctx);});assert!(app.profile.is_none());
        app.show_profile_at(&user,egui::Rect::from_min_size(egui::pos2(340.,180.),Vec2::splat(30.)));
        let _=ctx.run(input(vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]),|ctx|{app.profile_popout(ctx);});assert!(app.profile.is_none());
    }
    #[test]
    fn stale_profile_response_cannot_replace_another_server_profile(){
        let ctx=egui::Context::default();let mut app=Eclipse::with_context(&ctx,true,None);let user=app.messages.front().unwrap().author.clone();
        app.guild=Some("111".into());app.show_profile_at(&user,egui::Rect::NOTHING);let old=app.profile_key(&user.id);
        app.guild=Some("222".into());app.show_profile_at(&user,egui::Rect::NOTHING);let current=app.profile_key(&user.id);
        app.feature_result(old.clone(),Ok(serde_json::json!({"guild_member_profile":{"bio":"Wrong server"}})));assert!(!app.features.contains_key(&old));
        app.feature_result(current.clone(),Ok(serde_json::json!({"guild_member_profile":{"bio":"Right server"}})));assert_eq!(app.features[&current]["guild_member_profile"]["bio"],"Right server");
    }
}
