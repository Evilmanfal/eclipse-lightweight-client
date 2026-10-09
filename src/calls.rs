//! Explicit call intent and Gateway signaling. Media and capture stay off the UI thread.
use crate::model::{Channel, User};
use discord_voice::{self as voice, Controls, Status};
use eframe::egui::{self, Vec2};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};
use voice_core::voice::{Secret, VoiceConnection};
use voice_model::Id;
use zeroize::Zeroizing;

enum Notice {
    State(Status),
    Error(&'static str),
    Ended,
    StreamEnded(bool, Result<(), &'static str>),
}
struct VideoFrame {
    user: u64,
    image: egui::ColorImage,
}
#[derive(Default)]
struct StreamConn {
    server: Option<Id>,
    channel: Option<Id>,
    token: Option<Zeroizing<String>>,
    endpoint: Option<String>,
}
struct Session {
    ptt_stop:Arc<AtomicBool>,
    runtime: tokio::runtime::Runtime,
    task: tokio::task::JoinHandle<()>,
    controls: tokio::sync::watch::Sender<Controls>,
    audio: Arc<Mutex<voice::audio::Audio>>,
    identity: Arc<voice::Identity>,
    camera: Option<voice::camera::Camera>,
    camera_send: mpsc::SyncSender<voice::camera_video::Frame>,
    generation: Arc<AtomicU64>,
    next_camera: u64,
    stream_playback: mpsc::SyncSender<voice::Frame>,
    screen: Option<voice::screen::Worker>,
    screen_video: Option<voice::screen::Video>,
    screen_task: Option<tokio::task::JoinHandle<()>>,
    watch_task: Option<tokio::task::JoinHandle<()>>,
    /// Discord's stream server details, kept apart for your own share and the stream you
    /// watch so both can run at once.
    share_conn: StreamConn,
    watch_conn: StreamConn,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.ptt_stop.store(true,Ordering::Release);
        self.generation.store(0, Ordering::Release);
        self.camera = None;
        self.screen = None;
        self.task.abort();
        if let Some(task) = &self.screen_task {
            task.abort();
        }
        if let Some(task) = &self.watch_task {
            task.abort();
        }
        if let Ok(audio) = self.audio.lock() {
            audio.set_controls(true, true);
            audio.set_ready(false);
        }
    }
}

pub struct Calls {
    prefs:crate::preferences::Preferences,
    ptt:Arc<AtomicU64>,
    /// Mute (1), deafen (2) and feedback-sound (4) state shared with the push-to-talk thread.
    control_bits:Arc<AtomicU8>,
    volumes:HashMap<u64,u16>,
    stream_volume:u16,
    pub chat:bool,
    ctx: egui::Context,
    channel: Option<Channel>,
    user: Option<User>,
    session_id: Option<Zeroizing<String>>,
    token: Option<Zeroizing<String>>,
    endpoint: Option<String>,
    media: Option<Session>,
    tx: crossbeam_channel::Sender<(u64, Notice)>,
    rx: crossbeam_channel::Receiver<(u64, Notice)>,
    active_epoch: Arc<AtomicU64>,
    participants: HashMap<String, User>,
    /// Participant ids in the order they joined, so newcomers appear on the right.
    join_order: Vec<String>,
    /// We were moved to another channel and still need its details from the app.
    moved: bool,
    /// Round trip to Discord's voice server, from the latest heartbeat.
    ping_ms: Option<u32>,
    speaking: Vec<u64>,
    frames: Arc<Mutex<HashMap<u64, VideoFrame>>>,
    textures: HashMap<u64, egui::TextureHandle>,
    /// The watched screen share at full resolution, separate from the small camera tiles.
    stream_frame: Arc<Mutex<Option<egui::ColorImage>>>,
    stream_texture: Option<egui::TextureHandle>,
    /// What you are sharing, from the capture preview (up to 640x360, about ten times a second).
    own_preview: Option<egui::TextureHandle>,
    /// Your own stream fills the stage instead of the participant tiles.
    pub view_own: bool,
    /// Preview pictures of other people's streams, by user id (fetched by the app).
    pub previews: HashMap<String, String>,
    /// When your next stream preview picture is due, and one waiting to be uploaded.
    upload_due: Option<Instant>,
    upload: Option<(String, String)>,
    /// The watched stream fills the whole call area (no participant strip).
    pub expanded: bool,
    fullscreen: bool,
    sink: voice::VideoSink,
    pub status: String,
    error: Option<String>,
    joined: Option<Instant>,
    muted: bool,
    deafened: bool,
    video: bool,
    ready: bool,
    join_announced: bool,
    server_muted: bool,
    server_deafened: bool,
    epoch: u64,
    share_audio: bool,
    share_picker: bool,
    /// The Screens tab (rather than Applications) is showing in the share picker.
    picker_screens: bool,
    /// Someone to start watching once the call connects (Watch Stream from outside the call).
    pending_watch: Option<String>,
    /// A stream we just told Discord we left, so its STREAM_DELETE reply can't cancel a re-watch.
    left_stream: Option<(String, Instant)>,
    sources: Vec<voice::screen::Source>,
    source: usize,
    stream_key: Option<String>,
    watch_key: Option<String>,
    streams: Vec<(String, String)>,
    outbound: Vec<Value>,
}
fn id(value: &str) -> Result<Id, &'static str> {
    value
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0)
        .map(Id)
        .ok_or("Invalid call ID")
}
fn voice_payload(channel: Option<&Channel>, muted: bool, deafened: bool, video: bool) -> Value {
    json!({"op":4,"d":{"guild_id":channel.and_then(|c|c.guild_id.as_deref()),"channel_id":channel.map(|c|c.id.as_str()),"self_mute":muted,"self_deaf":deafened,"self_video":video}})
}
impl Calls {
    pub fn intent(&self) -> (Arc<AtomicU64>, u64) {
        (self.active_epoch.clone(), self.epoch)
    }
    pub fn new(ctx: egui::Context) -> Self {
        let (tx, rx) = crossbeam_channel::bounded(32);
        let active_epoch = Arc::new(AtomicU64::new(0));
        let sink_epoch = active_epoch.clone();
        let frames = Arc::new(Mutex::new(HashMap::new()));
        let latest = frames.clone();
        let repaint = ctx.clone();
        let sink: voice::VideoSink = Arc::new(move |frame| {
            let epoch = sink_epoch.load(Ordering::Acquire);
            if frame.width == 0
                || frame.height == 0
                || frame.rgba.len() != frame.width as usize * frame.height as usize * 4
            {
                return;
            }
            // Four visible remote videos, at most 640x360 each, one replaceable frame per user.
            if let Some(image) =
                image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba.to_vec())
            {
                let image = image::DynamicImage::ImageRgba8(image)
                    .thumbnail(640, 360)
                    .to_rgba8();
                if let Ok(mut latest) = latest.lock() {
                    if sink_epoch.load(Ordering::Acquire) != epoch {
                        return;
                    }
                    if latest.len() < 5 || latest.contains_key(&frame.user) {
                        latest.insert(
                            frame.user,
                            VideoFrame {
                                user: frame.user,
                                image: egui::ColorImage::from_rgba_unmultiplied(
                                    [image.width() as usize, image.height() as usize],
                                    image.as_raw(),
                                ),
                            },
                        );
                    }
                }
                repaint.request_repaint();
            }
        });
        Self {
            prefs:Default::default(),ptt:Arc::new(AtomicU64::new(0)),control_bits:Arc::new(AtomicU8::new(0)),volumes:HashMap::new(),stream_volume:100,chat:false,
            ctx,
            active_epoch,
            participants: HashMap::new(),
            join_order: Vec::new(),
            moved: false,
            ping_ms: None,
            speaking: vec![],
            channel: None,
            user: None,
            session_id: None,
            token: None,
            endpoint: None,
            media: None,
            tx,
            rx,
            frames,
            textures: HashMap::new(),
            stream_frame: Arc::new(Mutex::new(None)),
            stream_texture: None,
            own_preview: None,
            view_own: false,
            previews: HashMap::new(),
            upload_due: None,
            upload: None,
            expanded: false,
            fullscreen: false,
            sink,
            status: "No active call".into(),
            error: None,
            joined: None,
            muted: false,
            deafened: false,
            video: false,
            ready: false,
            join_announced: false,
            server_muted: false,
            server_deafened: false,
            epoch: 0,
            share_audio: false,
            share_picker: false,
            picker_screens: true,
            pending_watch: None,
            left_stream: None,
            sources: vec![],
            source: 0,
            stream_key: None,
            watch_key: None,
            streams: vec![],
            outbound: vec![],
        }
    }
    pub fn join(
        &mut self,
        channel: &Channel,
        user: &User,
        video: bool,
    ) -> Result<Value, &'static str> {
        id(&channel.id)?;
        id(&user.id)?;
        if let Some(guild) = &channel.guild_id {
            id(guild)?;
        }
        if self.channel.is_some() {
            return Err("Hang up the current call before starting another.");
        }
        let (muted,deafened)=(self.muted,self.deafened);
        self.disconnect();
        self.muted=muted;self.deafened=deafened;self.chat=false;
        self.participants.insert(user.id.clone(), user.clone());
        for peer in &channel.recipients {
            self.participants.insert(peer.id.clone(), peer.clone());
        }
        self.channel = Some(channel.clone());
        self.user = Some(user.clone());
        self.video = video;
        self.joined = Some(Instant::now());
        self.status = "Connecting to voice server…".into();
        Ok(voice_payload(
            self.channel.as_ref(),
            self.muted,
            self.deafened,
            self.video,
        ))
    }
    pub fn disconnect(&mut self) {
        if self.join_announced{crate::sounds::play(crate::sounds::Cue::Leave,self.prefs.ui_sounds);}
        self.join_announced=false;
        self.epoch = self.epoch.wrapping_add(1);
        self.active_epoch.store(self.epoch, Ordering::Release);
        self.participants.clear();
        self.join_order.clear();
        self.speaking.clear();
        self.ping_ms = None;
        self.pending_watch = None;
        self.left_stream = None;
        self.media = None;
        self.channel = None;
        self.user = None;
        self.session_id = None;
        self.token = None;
        self.endpoint = None;
        self.joined = None;
        self.ready = false;
        self.video = false;
        self.server_muted = false;
        self.server_deafened = false;
        self.muted = false;
        self.deafened = false;
        self.stream_key = None;
        self.watch_key = None;
        self.streams.clear();
        self.textures.clear();
        self.share_picker = false;
        self.error = None;
        if let Ok(mut f) = self.frames.lock() {
            f.clear();
        }
        while self.rx.try_recv().is_ok() {}
        self.status = "Call ended".into();
    }
    fn hang_up(&mut self) {
        let payload = voice_payload(self.channel.as_ref(), true, true, false);
        if let Some(key) = self.stream_key.clone() {
            self.outbound.push(json!({"op":19,"d":{"stream_key":key}}));
        }
        self.disconnect();
        let mut payload = payload;
        payload["d"]["channel_id"] = Value::Null;
        self.outbound.push(payload);
    }
    pub fn signal(&mut self, kind: &str, data: &Value) {
        let Some(channel) = self.channel.clone() else {
            return;
        };
        let Some(user) = &self.user else { return };
        match kind {
            "VOICE_STATE_UPDATE" => {
                if let Some(remote) = data["user_id"].as_str() {
                    if data["channel_id"].as_str() == Some(&channel.id) {
                        if let Ok(user) =
                            serde_json::from_value::<User>(data["member"]["user"].clone())
                        {
                            if self.participants.len() < 64 {
                                self.participants.insert(user.id.clone(), user);
                            }
                        }
                    } else {
                        self.participants.remove(remote);
                    }
                }
                if data["user_id"].as_str() == Some(&user.id) {
                    if data["channel_id"].as_str() != Some(&channel.id) {
                        match data["channel_id"].as_str() {
                            // Moved by a moderator or a bot (such as "join to create" channels):
                            // follow to the new channel like Discord, instead of hanging up.
                            Some(moved) if self.session_id.is_some() && channel.guild_id.is_some() => self.follow_move(moved, data),
                            _ => if self.session_id.is_some() { self.disconnect(); },
                        }
                        return;
                    }
                    if self.media.is_some()
                        && self.session_id.as_ref().is_some_and(|old| {
                            data["session_id"]
                                .as_str()
                                .is_some_and(|new| new != old.as_str())
                        })
                    {
                        self.disconnect();
                        self.error = Some("The call moved to another Discord session.".into());
                        return;
                    }
                    self.session_id = data["session_id"]
                        .as_str()
                        .map(|s| Zeroizing::new(s.into()));
                    self.server_muted = data["mute"] == true || data["suppress"] == true;
                    self.server_deafened = data["deaf"] == true;
                    self.apply_controls();
                }
                if let Some(remote) = data["user_id"].as_str().and_then(|s| s.parse::<u64>().ok()) {
                    if data["channel_id"].as_str() != Some(&channel.id)
                        || data["self_video"] != true
                    {
                        self.textures.remove(&remote);
                        if let Ok(mut frames) = self.frames.lock() {
                            frames.remove(&remote);
                        }
                    }
                }
                if data["channel_id"].as_str() == Some(&channel.id) {
                    if let Some(remote) = data["user_id"].as_str() {
                        self.streams.retain(|(_, u)| u != remote);
                        if data["self_stream"] == true
                            && remote != user.id
                            && self.streams.len() < 64
                        {
                            let key = stream_key(&channel, remote);
                            self.streams.push((key, remote.into()));
                        }
                    }
                }
            }
            "CALL_CREATE" | "CALL_UPDATE" => {
                if data["channel_id"].as_str() == Some(&channel.id) {
                    if let Some(states) = data["voice_states"].as_array() {
                        for state in states.iter().take(64) {
                            self.signal("VOICE_STATE_UPDATE", state);
                        }
                    }
                }
            }
            "VOICE_SERVER_UPDATE" => {
                let matches = if let Some(guild) = channel.guild_id.as_deref() {
                    data["guild_id"].as_str() == Some(guild)
                } else {
                    data["channel_id"].as_str() == Some(&channel.id)
                };
                if matches {
                    if self.media.is_some() {
                        self.media = None;
                        self.ready = false;
                        self.epoch = self.epoch.wrapping_add(1);
                        self.active_epoch.store(self.epoch, Ordering::Release);
                        self.stream_key = None;
                        self.watch_key = None;
                        self.textures.clear();
                    }
                    self.token = data["token"].as_str().map(|s| Zeroizing::new(s.into()));
                    self.endpoint = data["endpoint"].as_str().map(str::to_owned);
                }
            }
            "STREAM_CREATE" | "STREAM_SERVER_UPDATE" => {
                let key = data["stream_key"].as_str();
                if key.is_some()
                    && (key == self.stream_key.as_deref() || key == self.watch_key.as_deref())
                {
                    let sharing = key == self.stream_key.as_deref();
                    if let Some(media) = &mut self.media {
                        let conn = if sharing { &mut media.share_conn } else { &mut media.watch_conn };
                        if kind == "STREAM_CREATE" {
                            conn.server = data["rtc_server_id"].as_str().and_then(|s| id(s).ok());
                            conn.channel = data["rtc_channel_id"].as_str().and_then(|s| id(s).ok());
                        } else {
                            conn.token = data["token"].as_str().map(|s| Zeroizing::new(s.into()));
                            conn.endpoint = data["endpoint"].as_str().map(str::to_owned);
                        }
                    }
                    self.start_stream();
                }
            }
            "STREAM_DELETE" => {
                let key = data["stream_key"].as_str();
                if key == self.stream_key.as_deref() {
                    self.stop_share();
                }
                // Discord confirming that we left: harmless, even if we are already watching again.
                let ours = self.left_stream.as_ref().is_some_and(|(left, at)| Some(left.as_str()) == key && at.elapsed() < Duration::from_secs(5));
                if ours {
                    self.left_stream = None;
                } else if key == self.watch_key.as_deref() {
                    if let Some(media) = &mut self.media {
                        if let Some(task) = media.watch_task.take() {
                            task.abort();
                        }
                    }
                    self.watch_key = None;
                }
            }
            "CALL_DELETE" => {
                if data["channel_id"].as_str() == Some(&channel.id) {
                    self.disconnect();
                }
            }
            _ => {}
        }
        self.resume_media();
    }
    /// Starts the voice connection once Discord has supplied the session, token and endpoint.
    fn resume_media(&mut self) {
        if self.media.is_none()
            && self.session_id.is_some()
            && self.token.is_some()
            && self.endpoint.is_some()
        {
            if let Err(error) = self.start_media() {
                self.error = Some(error.into());
                self.hang_up();
                self.error = Some(error.into());
            }
        }
    }
    /// Discord moved us to another voice channel of the same server: rebuild the voice
    /// connection for it. Your screen share and any watched stream end, as in Discord.
    fn follow_move(&mut self, moved: &str, data: &Value) {
        if id(moved).is_err() { return; }
        self.stop_share();
        self.stop_watching();
        self.media = None;
        self.ready = false;
        self.ping_ms = None;
        self.epoch = self.epoch.wrapping_add(1);
        self.active_epoch.store(self.epoch, Ordering::Release);
        self.textures.clear();
        self.streams.clear();
        self.speaking.clear();
        let me = self.user.as_ref().map(|u| u.id.clone()).unwrap_or_default();
        self.participants.retain(|id, _| *id == me);
        self.join_order.clear();
        if let Some(channel) = &mut self.channel {
            channel.id = moved.to_owned();
            channel.name = None;
        }
        self.moved = true;
        self.status = "Moving to another voice channel…".into();
        if let Some(session) = data["session_id"].as_str() { self.session_id = Some(Zeroizing::new(session.into())); }
        self.resume_media();
    }
    /// After a move, the app fills in the new channel's name and details.
    pub fn moved_channel(&self) -> Option<&str> { self.moved.then(|| self.channel.as_ref().map(|c| c.id.as_str())).flatten() }
    pub fn set_moved_channel(&mut self, channel: Channel) {
        if self.channel.as_ref().is_some_and(|c| c.id == channel.id) { self.channel = Some(channel); self.moved = false; }
    }
    fn credentials(&self) -> Result<VoiceConnection, &'static str> {
        let channel = self.channel.as_ref().ok_or("Call ended")?;
        let user = self.user.as_ref().ok_or("Call ended")?;
        Ok(VoiceConnection {
            channel: id(&channel.id)?,
            guild: channel.guild_id.as_deref().map(id).transpose()?,
            user: id(&user.id)?,
            peer: if channel.kind == 1 {
                channel.recipients.first().map(|u| id(&u.id)).transpose()?
            } else {
                None
            },
            session: Secret::new(
                self.session_id
                    .as_ref()
                    .ok_or("Waiting for voice session")?
                    .to_string(),
            )?,
            token: Secret::new(
                self.token
                    .as_ref()
                    .ok_or("Waiting for voice token")?
                    .to_string(),
            )?,
            endpoint: self.endpoint.clone().ok_or("Waiting for voice endpoint")?,
            request: 1,
        })
    }
    fn start_media(&mut self) -> Result<(), &'static str> {
        let credentials = self.credentials()?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map_err(|_| "Could not start media runtime")?;
        let epoch = self.epoch;
        let (stream_playback, stream_audio) = mpsc::sync_channel(8);
        let (capture_send, capture) = mpsc::sync_channel(8);
        let (playback_send, playback) = mpsc::sync_channel(8);
        let (camera_send, camera) = mpsc::sync_channel(2);
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        let audio = Arc::new(Mutex::new(voice::audio::Audio::start(
            voice::audio::Devices{input:self.prefs.input.clone(),output:self.prefs.output.clone()},
            capture_send,
            playback,
            move |result| {
                if let Err(error) = result {
                    let _ = tx.try_send((epoch, Notice::Error(error)));
                }
                ctx.request_repaint();
            },
        )?));
        // Set the microphone gate before transport can report media readiness.
        if let Ok(audio)=audio.lock(){audio.set_input_enabled(true);audio.set_controls(self.muted||self.server_muted,self.deafened||self.server_deafened);audio.set_hold(!hotkey_open(self.ptt.load(Ordering::Acquire),key_down));audio.set_gain(self.prefs.input_gain,self.prefs.output_gain);audio.set_processing(effective_processing(&self.prefs));}
        let identity = voice::Identity::generate();
        let shared_identity = identity.clone();
        let (control_send, controls) = tokio::sync::watch::channel(Controls::default());
        let (tx, ctx, sink, audio_gate) = (
            self.tx.clone(),
            self.ctx.clone(),
            self.valid_sink(),
            audio.clone(),
        );
        let audio_after = audio_gate.clone();
        let task = runtime.spawn(async move {
            let status_tx = tx.clone();
            let result = voice::run_with_identity(
                credentials,
                capture,
                playback_send,
                controls,
                Some(camera),
                Some(sink),
                Some(stream_audio),
                move |status| {
                    if let Ok(audio) = audio_gate.lock() {
                        match status {
                            Status::Ready { .. } | Status::WaitingForPeer => audio.set_ready(true),
                            Status::Securing
                            | Status::Connecting
                            | Status::Discovering
                            | Status::TransportReady => audio.set_ready(false),
                            _ => {}
                        }
                    }
                    let _ = status_tx.try_send((epoch, Notice::State(status)));
                    ctx.request_repaint();
                    Ok(())
                },
                shared_identity,
            )
            .await;
            if let Ok(audio) = audio_gate_after(&audio_after) {
                audio.set_ready(false);
            }
            let _ = tx.try_send((
                epoch,
                match result {
                    Ok(()) => Notice::Ended,
                    Err(e) => Notice::Error(e),
                },
            ));
        });
        self.media = Some(Session {
            ptt_stop:Arc::new(AtomicBool::new(false)),
            runtime,
            task,
            controls: control_send,
            audio,
            identity,
            camera: None,
            camera_send,
            generation: Arc::new(AtomicU64::new(0)),
            next_camera: 0,
            stream_playback,
            screen: None,
            screen_video: None,
            screen_task: None,
            watch_task: None,
            share_conn: StreamConn::default(),
            watch_conn: StreamConn::default(),
        });
        if let Some(media)=&self.media {
            let weak=Arc::downgrade(&media.audio);let stop=media.ptt_stop.clone();let config=self.ptt.clone();let bits=self.control_bits.clone();
            // Push to talk holds back the always-open microphone without muting it: muting reset
            // echo cancellation and noise suppression on every press, so each press began with
            // static and short phrases sounded muffled. Toggling the device itself was slower still.
            std::thread::spawn(move||{let mut last=None;while !stop.load(Ordering::Acquire){
                let Some(shared)=weak.upgrade()else{return;};
                let settings=config.load(Ordering::Acquire);let open=hotkey_open(settings,key_down);let state=bits.load(Ordering::Acquire);let(muted,deafened)=(state&1!=0,state&2!=0);
                if let Ok(audio)=shared.lock(){audio.set_controls(muted,deafened);audio.set_hold(!open);}drop(shared);
                if settings&(1<<16)!=0&&!muted&&!deafened&&last.is_some_and(|was|was!=open){crate::sounds::play(if open{crate::sounds::Cue::PttOn}else{crate::sounds::Cue::PttOff},state&4!=0);}
                last=Some(open);std::thread::sleep(Duration::from_millis(4));
            }});
        }
        self.configure_audio();
        self.apply_controls();
        if self.video {
            self.toggle_camera(true)?;
        }
        Ok(())
    }
    fn valid_sink(&self) -> voice::VideoSink {
        let epoch = self.epoch;
        let active = self.active_epoch.clone();
        let sink = self.sink.clone();
        Arc::new(move |frame| {
            if active.load(Ordering::Acquire) == epoch {
                sink(frame);
            }
        })
    }
    fn toggle_camera(&mut self, enabled: bool) -> Result<(), &'static str> {
        let own = self
            .user
            .as_ref()
            .and_then(|user| user.id.parse::<u64>().ok())
            .unwrap_or(0);
        self.textures.remove(&own);
        if let Ok(mut frames) = self.frames.lock() {
            frames.remove(&own);
        }
        let media = self.media.as_mut().ok_or("Wait for the call to connect")?;
        media.camera = None;
        if enabled {
            media.next_camera = media
                .next_camera
                .checked_add(1)
                .ok_or("Rejoin the call to reset the camera")?;
            let generation = media.next_camera;
            media.generation.store(generation, Ordering::Release);
            let (tx, active, ctx, latest) = (
                media.camera_send.clone(),
                media.generation.clone(),
                self.ctx.clone(),
                self.frames.clone(),
            );
            let timestamp = AtomicU64::new(0);
            let callback = Arc::new(move |frame: voice::camera::Frame| {
                if active.load(Ordering::Acquire) == generation {
                    let _ = tx.try_send(voice::camera_video::Frame {
                        generation,
                        timestamp: timestamp.fetch_add(6000, Ordering::Relaxed) as u32,
                        data: frame.h264,
                    });
                    if let Some(rgb) = image::RgbImage::from_raw(
                        voice::camera::WIDTH as u32,
                        voice::camera::HEIGHT as u32,
                        frame.rgb,
                    ) {
                        let rgb = image::DynamicImage::ImageRgb8(rgb)
                            .thumbnail(320, 240)
                            .to_rgba8();
                        if let Ok(mut frames) = latest.lock() {
                            if active.load(Ordering::Acquire) == generation
                                && (frames.len() < 5 || frames.contains_key(&own))
                            {
                                frames.insert(
                                    own,
                                    VideoFrame {
                                        user: own,
                                        image: egui::ColorImage::from_rgba_unmultiplied(
                                            [rgb.width() as usize, rgb.height() as usize],
                                            rgb.as_raw(),
                                        ),
                                    },
                                );
                            }
                        }
                    }
                }
            });
            media.camera = Some(voice::camera::Camera::start(
                None,
                callback,
                Arc::new(move || ctx.request_repaint()),
            )?);
            media.controls.send_modify(|c| c.camera = generation);
        } else {
            media.generation.store(0, Ordering::Release);
            media.controls.send_modify(|c| c.camera = 0);
        }
        self.video = enabled;
        Ok(())
    }
    fn stop_share(&mut self) {
        if let Some(media) = &mut self.media {
            media.screen = None;
            media.screen_video = None;
            if let Some(task) = media.screen_task.take() {
                task.abort();
            }
            media.share_conn = StreamConn::default();
        }
        self.stream_key = None;
        self.own_preview = None;
        self.view_own = false;
        self.upload_due = None;
        self.upload = None;
    }
    fn start_share(&mut self) -> Result<Value, &'static str> {
        let source = self
            .sources
            .get(self.source)
            .ok_or("Choose a screen or window")?
            .id;
        let media = self.media.as_mut().ok_or("Wait for the call to connect")?;
        let ctx = self.ctx.clone();
        let (worker, video) = voice::screen::Worker::start(
            voice::screen::Settings {
                source,
                width: if self.prefs.screen_height==1080{1920}else{1280},
                height: self.prefs.screen_height.min(1080),
                fps: self.prefs.screen_fps,
                cursor: true,
                audio: self.share_audio,
            },
            move || ctx.request_repaint(),
        )?;
        media.screen = Some(worker);
        media.screen_video = Some(video);
        media.share_conn = StreamConn::default();
        let channel = self.channel.as_ref().ok_or("Call ended")?;
        let user = self.user.as_ref().ok_or("Call ended")?;
        self.stream_key = Some(stream_key(channel, &user.id));
        self.upload_due = Some(Instant::now() + Duration::from_secs(3));
        Ok(
            json!({"op":18,"d":{"type":if channel.guild_id.is_some(){"guild"}else{"call"},"guild_id":channel.guild_id,"channel_id":channel.id,"preferred_region":null}}),
        )
    }
    fn start_stream(&mut self) {
        let Some(media) = &mut self.media else { return };
        let (Some(session), Some(user)) = (self.session_id.as_ref(), self.user.as_ref()) else { return };
        let credentials = |conn: &StreamConn| -> Option<VoiceConnection> {
            let (Some(server), Some(channel), Some(token), Some(endpoint)) = (conn.server, conn.channel, conn.token.as_ref(), conn.endpoint.as_ref()) else { return None };
            Some(VoiceConnection {
                channel,
                guild: Some(server),
                user: id(&user.id).ok()?,
                peer: None,
                session: Secret::new(session.to_string()).ok()?,
                token: Secret::new(token.to_string()).ok()?,
                endpoint: endpoint.clone(),
                request: 1,
            })
        };
        let (tx, ctx, epoch) = (self.tx.clone(), self.ctx.clone(), self.epoch);
        if let (true, Some(credentials)) = (media.screen_video.is_some(), credentials(&media.share_conn)) {
            let video = media.screen_video.take().expect("checked above");
            let (identity, tx, ctx) = (media.identity.clone(), tx.clone(), ctx.clone());
            media.screen_task = Some(media.runtime.spawn(async move {
                let result = voice::run_stream(credentials, identity, video, move |_| {
                    ctx.request_repaint();
                    Ok(())
                })
                .await;
                let _ = tx.try_send((epoch, Notice::StreamEnded(false, result)));
            }));
        }
        if let (true, Some(credentials)) = (self.watch_key.is_some() && media.watch_task.is_none(), credentials(&media.watch_conn)) {
            let sink = stream_sink(self.stream_frame.clone(), self.active_epoch.clone(), self.epoch, self.ctx.clone());
            let (identity, playback) = (media.identity.clone(), media.stream_playback.clone());
            media.watch_task = Some(media.runtime.spawn(async move {
                let result =
                    voice::watch_stream(credentials, identity, sink, Some(playback), move |_| {
                        ctx.request_repaint();
                        Ok(())
                    })
                    .await;
                let _ = tx.try_send((epoch, Notice::StreamEnded(true, result)));
            }));
        }
    }
    pub fn poll(&mut self) {
        for (epoch, notice) in self.rx.try_iter().take(32).collect::<Vec<_>>() {
            if epoch != self.epoch {
                continue;
            }
            match notice {
                Notice::State(status) => {
                    self.status = match status {
                        Status::Connecting => "Connecting media…".into(),
                        Status::Discovering => "Checking voice network…".into(),
                        Status::TransportReady | Status::Securing => {
                            self.ready = false;
                            "Securing media…".into()
                        }
                        Status::WaitingForPeer => {
                            self.ready = true;
                            "Connected · waiting for others".into()
                        }
                        Status::Ready { privacy_code: _ } => {
                            self.ready = true;
                            "Voice connected · encrypted".into()
                        }
                        Status::RemoteAudio => continue,
                        Status::Speaking(users) => {
                            self.speaking = users.iter().copied().filter(|id| *id != 0).collect();
                            continue;
                        }
                        Status::CameraAvailable(_) => continue,
                        Status::Ping(ms) => { self.ping_ms = Some(ms); continue; }
                    };
                    if self.ready&&!self.join_announced{self.join_announced=true;crate::sounds::play(crate::sounds::Cue::Join,self.prefs.ui_sounds);}
                }
                Notice::Error(error) => {
                    self.hang_up();
                    self.error = Some(error.into());
                }
                Notice::Ended => self.hang_up(),
                Notice::StreamEnded(watching, result) => {
                    if watching {
                        if let Some(media) = &mut self.media {
                            media.watch_task = None;
                        }
                        self.watch_key = None;
                    } else {
                        if let Some(key) = self.stream_key.clone() {
                            self.outbound.push(json!({"op":19,"d":{"stream_key":key}}));
                        }
                        self.stop_share();
                    }
                    if let Err(error) = result {
                        self.error = Some(error.into());
                    }
                }
            }
        }
        if self.channel.is_some()
            && self.media.is_none()
            && self
                .joined
                .is_some_and(|at| at.elapsed() > Duration::from_secs(30))
        {
            self.hang_up();
            self.error =
                Some("Discord did not provide a voice connection. Try calling again.".into());
        }
        if self
            .media
            .as_ref()
            .is_some_and(|media| media.task.is_finished())
        {
            self.hang_up();
            self.error = Some("Media connection ended. Rejoin to try again.".into());
        }
        if let Some(image) = self.media.as_ref().and_then(|m| m.screen.as_ref()).and_then(|screen| screen.take_preview()) {
            // Your stream's preview picture for others, refreshed every minute while you share.
            if self.prefs.stream_preview && self.upload_due.is_some_and(|due| Instant::now() >= due) {
                if let Some(key) = self.stream_key.clone() {
                    if let Some(thumbnail) = preview_jpeg(&image) { self.upload = Some((key, thumbnail)); }
                }
                self.upload_due = Some(Instant::now() + Duration::from_secs(60));
            }
            let image = egui::ColorImage::from_rgba_unmultiplied([image.width() as usize, image.height() as usize], image.as_raw());
            match &mut self.own_preview {
                Some(texture) => texture.set(image, egui::TextureOptions::LINEAR),
                None => self.own_preview = Some(self.ctx.load_texture("own-stream", image, egui::TextureOptions::LINEAR)),
            }
        }
        if let Some(media) = &mut self.media {
            if let Some(error) = media.camera.as_ref().and_then(|c| c.error()) {
                self.error = Some(error.into());
                media.camera = None;
                media.controls.send_modify(|c| c.camera = 0);
                self.video = false;
            }
            if let Some(result) = media.screen.as_ref().and_then(|s| s.result()) {
                if let Err(error) = result {
                    self.error = Some(error.into());
                }
                if let Some(key) = self.stream_key.clone() {
                    self.outbound.push(json!({"op":19,"d":{"stream_key":key}}));
                }
                self.stop_share();
            }
        }
        if self.watch_key.is_none() {
            if self.stream_texture.take().is_some() { if let Ok(mut f) = self.stream_frame.lock() { *f = None; } }
            self.expanded = false;
            if self.fullscreen { self.fullscreen = false; self.ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false)); }
        } else if let Some(image) = self.stream_frame.lock().ok().and_then(|mut f| f.take()) {
            match &mut self.stream_texture {
                Some(texture) if texture.size() == image.size => texture.set(image, egui::TextureOptions::LINEAR),
                _ => self.stream_texture = Some(self.ctx.load_texture("watched-stream", image, egui::TextureOptions::LINEAR)),
            }
        }
        if let Ok(mut frames) = self.frames.lock() {
            for frame in frames.drain().map(|(_, f)| f) {
                if let Some(texture) = self.textures.get_mut(&frame.user) {
                    texture.set(frame.image, egui::TextureOptions::LINEAR);
                } else if self.textures.len() < 5 {
                    self.textures.insert(
                        frame.user,
                        self.ctx.load_texture(
                            format!("remote-{}", frame.user),
                            frame.image,
                            egui::TextureOptions::LINEAR,
                        ),
                    );
                }
            }
        }
    }

    pub fn stage(&mut self,ui:&mut egui::Ui,images:&mut crate::assets::Images){
        use crate::widgets::{control,Control};
        let Some(channel)=self.channel.clone()else{return;};
        egui::Frame::NONE.fill(egui::Color32::from_gray(16)).corner_radius(12).inner_margin(20).show(ui,|ui|{
            ui.set_min_size(ui.available_size());
            ui.horizontal(|ui|{ui.strong(channel.label());ui.weak(&self.status);ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if ui.button("Show chat").clicked(){self.chat=true;}});});
            let tiles_height=(ui.available_height()-110.).max(100.);
            let mut watch=None;let mut stop=false;
            egui::ScrollArea::vertical().id_salt("call-tiles").max_height(tiles_height).show(ui,|ui|{
                let participants=self.in_join_order();
                let own_view=self.view_own&&self.stream_key.is_some();
                let focus=if own_view{Some(String::new())}else{self.watching().map(str::to_owned)};
                if let Some(streamer)=focus{
                    // The watched stream takes the stage, like Discord; everyone else sits in a strip below
                    // unless the stream is expanded.
                    let width=ui.available_width();
                    let height=if self.expanded{(tiles_height-44.).max(160.)}else{(width*0.5625).min(tiles_height-150.).max(160.)};
                    let (rect,response)=ui.allocate_exact_size(Vec2::new(width,height),egui::Sense::click());
                    ui.painter().rect_filled(rect,8,egui::Color32::BLACK);
                    if let Some(texture)=if own_view{&self.own_preview}else{&self.stream_texture}{
                        let size=texture.size_vec2();let scale=(rect.width()/size.x.max(1.)).min(rect.height()/size.y.max(1.));
                        egui::Image::new(texture).corner_radius(8).paint_at(ui,egui::Rect::from_center_size(rect.center(),size*scale));
                    }else{
                        ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,if own_view{"Starting your stream…"}else{"Connecting to stream…"},egui::FontId::proportional(15.),egui::Color32::GRAY);
                    }
                    let name=if own_view{"Your stream · preview".to_owned()}else{self.participants.get(&streamer).map(|u|u.name().to_owned()).unwrap_or_default()};
                    live_badge(ui,rect.left_top()+Vec2::new(12.,12.));
                    if response.double_clicked(){self.toggle_fullscreen();}
                    if ui.rect_contains_pointer(rect){
                        ui.painter().text(rect.left_bottom()+Vec2::new(12.,-12.),egui::Align2::LEFT_BOTTOM,name,egui::FontId::proportional(14.),egui::Color32::WHITE);
                        let button=egui::Rect::from_min_size(egui::pos2(rect.right()-128.,rect.top()+10.),Vec2::new(116.,30.));
                        if own_view{if overlay_button(ui,button,egui::Id::new("close-own-preview"),"Close Preview",egui::Color32::from_gray(70)){self.view_own=false;}}
                        else if overlay_button(ui,button,egui::Id::new("stop-watching"),"Stop Watching",egui::Color32::from_rgb(218,55,60)){stop=true;}
                        let full=egui::Rect::from_min_size(rect.right_bottom()-Vec2::new(46.,46.),Vec2::splat(34.));
                        if icon_button(ui,full,egui::Id::new("stream-fullscreen"),if self.fullscreen{"Exit full screen"}else{"Full screen"},|p,c,s|{for (dx,dy) in [(-1.,-1.),(1.,-1.),(-1.,1.),(1.,1.)]{let corner=c+Vec2::new(dx*8.,dy*8.);p.line_segment([corner,corner-Vec2::new(dx*5.,0.)],s);p.line_segment([corner,corner-Vec2::new(0.,dy*5.)],s);}}){self.toggle_fullscreen();}
                        let grow=egui::Rect::from_min_size(full.min-Vec2::new(42.,0.),Vec2::splat(34.));
                        let expanded=self.expanded;
                        if icon_button(ui,grow,egui::Id::new("stream-expand"),if expanded{"Show participants"}else{"Enlarge stream"},|p,c,s|{let r=egui::Rect::from_center_size(c,Vec2::new(18.,12.));p.rect_stroke(r,2,s,egui::StrokeKind::Middle);if expanded{p.line_segment([r.left_bottom()+Vec2::new(0.,4.),r.right_bottom()+Vec2::new(0.,4.)],s);}}){self.expanded=!self.expanded;}
                    }
                    if !own_view{ui.horizontal(|ui|{let max=if self.prefs.volume_booster{1000}else{200};if ui.add(egui::Slider::new(&mut self.stream_volume,0..=max).text("Stream volume %")).changed(){self.apply_controls();}});}
                    ui.add_space(8.);
                    if !self.expanded{centered_rows(ui,participants.len(),Vec2::new(160.,90.),10.,|ui,index,rect|{if self.tile(ui,images,&participants[index],rect,true){watch=Some(participants[index].id.clone());}});}
                }else{
                    // Two tiles per row at most (one alone fills half the width), every row centered.
                    let width=((ui.available_width()-12.)/2.).max(150.);
                    centered_rows(ui,participants.len(),Vec2::new(width,(width*0.5625).clamp(120.,280.)),12.,|ui,index,rect|{if self.tile(ui,images,&participants[index],rect,false){watch=Some(participants[index].id.clone());}});
                }
            });
            if stop{self.stop_watching();}
            if let Some(user)=watch{
                if self.user.as_ref().is_some_and(|u|u.id==user){self.view_own=true;}
                else{self.view_own=false;if let Err(error)=self.watch(&user){self.error=Some(error.into());}}
            }
            ui.add_space((ui.available_height()-96.).max(8.));
            ui.horizontal(|ui|{
                ui.add_space(((ui.available_width()-320.)/2.).max(0.));
                if control(ui,Control::Mic,self.muted,48.,"Mute / unmute").clicked(){self.toggle_mute();}
                if control(ui,Control::Headphones,self.deafened,48.,"Deafen / undeafen").clicked(){self.toggle_deafen();}
                if control(ui,Control::Camera,self.video,48.,"Camera on / off").clicked(){if let Err(e)=self.toggle_camera(!self.video){self.error=Some(e.into());}else{self.update_controls();}}
                if control(ui,Control::Screen,self.stream_key.is_some(),48.,"Share your screen").clicked(){if self.stream_key.is_some(){if let Some(key)=self.stream_key.clone(){self.outbound.push(json!({"op":19,"d":{"stream_key":key}}));}self.stop_share();}else if self.ready{match voice::screen::sources(){Ok(sources)=>{self.sources=sources;self.picker_screens=true;self.source=self.sources.iter().position(|s|!matches!(s.id,voice::screen::SourceId::Window(_))).unwrap_or(0);self.share_picker=true;},Err(e)=>self.error=Some(e.into())}}else{self.error=Some("Wait for the call to connect before sharing.".into());}}
                if control(ui,Control::Hangup,false,48.,"Disconnect").clicked(){self.hang_up();}
            });
            ui.add_space(8.);ui.vertical_centered(|ui|{if self.prefs.push_to_talk{ui.small(if hotkey_open(self.ptt.load(Ordering::Acquire),key_down){"Push to talk · microphone open"}else{"Push to talk · hold your hotkey to speak"});}ui.weak("Native encrypted media · live Discord interoperability remains unverified");});
        });ui.ctx().request_repaint_after(Duration::from_millis(33));
    }
    pub fn toggle_fullscreen(&mut self){
        self.fullscreen=!self.fullscreen;self.expanded=self.fullscreen||self.expanded;
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
    }
    pub fn fullscreen(&self)->bool{self.fullscreen&&self.watch_key.is_some()}
    /// While watching from a text channel: the stream in a small movable window in the corner,
    /// like Discord's picture in picture. Clicking it returns to the call.
    fn picture_in_picture(&mut self,ctx:&egui::Context){
        let Some(texture)=self.stream_texture.clone() else{return};
        let name=self.watching().and_then(|id|self.participants.get(id)).map(|u|u.name().to_owned()).unwrap_or_default();
        let size=Vec2::new(320.,180.);
        let mut back=false;
        egui::Area::new(egui::Id::new("stream-pip")).order(egui::Order::Foreground).movable(true)
            .default_pos(ctx.screen_rect().right_bottom()-size-Vec2::new(24.,96.))
            .show(ctx,|ui|{
                let (rect,response)=ui.allocate_exact_size(size,egui::Sense::click());
                ui.painter().rect_filled(rect,10,egui::Color32::BLACK);
                let image=texture.size_vec2();let scale=(rect.width()/image.x.max(1.)).min(rect.height()/image.y.max(1.));
                egui::Image::new(&texture).corner_radius(10).paint_at(ui,egui::Rect::from_center_size(rect.center(),image*scale));
                ui.painter().rect_stroke(rect,10,egui::Stroke::new(1.0_f32,egui::Color32::from_gray(70)),egui::StrokeKind::Inside);
                live_badge(ui,rect.left_top()+Vec2::new(8.,8.));
                if response.hovered(){
                    ui.painter().rect_filled(rect,10,egui::Color32::from_black_alpha(90));
                    ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,"Click to return to the stream",egui::FontId::proportional(13.),egui::Color32::WHITE);
                    ui.painter().text(rect.left_bottom()+Vec2::new(10.,-8.),egui::Align2::LEFT_BOTTOM,name,egui::FontId::proportional(12.),egui::Color32::WHITE);
                }
                if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked(){back=true;}
            });
        if back{self.chat=false;}
    }
    /// Everyone in the call, oldest first: newcomers join on the right and leavers close the gap.
    fn in_join_order(&mut self)->Vec<User>{
        self.join_order.retain(|id|self.participants.contains_key(id));
        let mut new:Vec<_>=self.participants.keys().filter(|id|!self.join_order.contains(id)).cloned().collect();
        new.sort();
        self.join_order.extend(new);
        self.join_order.iter().filter_map(|id|self.participants.get(id).cloned()).collect()
    }
    /// One participant tile: camera or avatar, name, speaking outline, and for someone who is live,
    /// a LIVE badge and a Watch Stream button. Returns true when Watch Stream was clicked.
    fn tile(&mut self,ui:&mut egui::Ui,images:&mut crate::assets::Images,user:&User,rect:egui::Rect,small:bool)->bool{
        let id=user.id.parse::<u64>().unwrap_or(0);
        let own=self.user.as_ref().is_some_and(|u|u.id==user.id);
        let own_live=own&&self.stream_key.is_some();
        let live=(self.is_streaming(&user.id)&&self.watching()!=Some(user.id.as_str()))||(own_live&&!self.view_own);
        ui.painter().rect_filled(rect,8,egui::Color32::from_gray(39));
        let camera=if own_live{self.own_preview.as_ref()}else{self.textures.get(&id).filter(|_|self.watching()!=Some(user.id.as_str()))};
        let preview=(!own&&live).then(||self.previews.get(&user.id)).flatten().and_then(|url|images.texture(url,rect,ui.ctx()).map(|texture|(texture,images.dimensions(url,rect.size(),ui.ctx()))));
        if let Some((texture,size))=preview{
            egui::Image::new((texture,rect.size())).uv(crate::identity::cover_uv(size.unwrap_or(rect.size()),rect.size())).corner_radius(8).paint_at(ui,rect);
            ui.painter().rect_filled(rect,8,egui::Color32::from_black_alpha(80));
        }else if let Some(texture)=camera{egui::Image::new(texture).corner_radius(8).paint_at(ui,rect);}else{
            let radius=if small{24.}else{38.};
            let avatar=egui::Rect::from_center_size(rect.center()-Vec2::new(0.,if live&&!small{18.}else{0.}),Vec2::splat(radius*2.));ui.painter().circle_filled(avatar.center(),radius,egui::Color32::from_gray(65));
            if let Some(texture)=crate::assets::avatar_url(user,None,None).and_then(|url|images.texture(&url,avatar,ui.ctx())){egui::Image::new((texture,avatar.size())).corner_radius(radius).paint_at(ui,avatar);}else{ui.painter().text(avatar.center(),egui::Align2::CENTER_CENTER,user.name().chars().take(2).collect::<String>(),egui::FontId::proportional(radius*0.7),egui::Color32::WHITE);}
        }
        let speaking=self.is_speaking(&user.id);
        if speaking{ui.painter().rect_stroke(rect,8,egui::Stroke::new(2.0_f32,egui::Color32::from_rgb(35,165,90)),egui::StrokeKind::Inside);}
        ui.painter().text(rect.left_bottom()+Vec2::new(10.,-10.),egui::Align2::LEFT_BOTTOM,user.name(),egui::FontId::proportional(if small{12.}else{14.}),egui::Color32::WHITE);
        if !live{return false;}
        live_badge(ui,rect.left_top()+Vec2::new(10.,10.));
        let size=if own{if small{Vec2::new(128.,24.)}else{Vec2::new(150.,28.)}}else if small{Vec2::new(96.,24.)}else{Vec2::new(118.,28.)};
        let button=egui::Rect::from_center_size(if small{rect.center()}else{rect.center()+Vec2::new(0.,36.)},size);
        watch_pill(ui,button,egui::Id::new(("watch-stream",&user.id)),if own{"View Your Stream"}else{"Watch Stream"})
    }
    /// Offline preview screenshots: one peer is live, or the share picker is open.
    pub fn preview_live(&mut self,watching:bool){if let (Some(channel),Some(peer))=(self.channel.clone(),self.participants.keys().find(|id|Some(*id)!=self.user.as_ref().map(|u|&u.id)).cloned()){let key=stream_key(&channel,&peer);self.streams.push((key.clone(),peer));if watching{self.watch_key=Some(key);
        // A test pattern stands in for the stream picture offline.
        let image=egui::ColorImage::new([1280,720],(0..1280*720).map(|i|egui::Color32::from_rgb((i%1280*255/1280) as u8,(i/1280*255/720) as u8,160)).collect());
        if let Ok(mut frame)=self.stream_frame.lock(){*frame=Some(image);}}}}
    pub fn preview_picker(&mut self){
        use voice::screen::{Source,SourceId};
        self.sources=vec![Source{id:SourceId::Display(1),name:"Screen 1".into()},Source{id:SourceId::Display(2),name:"Screen 2".into()},Source{id:SourceId::Window(3),name:"Spotify Premium".into()},Source{id:SourceId::Window(4),name:"Visual Studio Code".into()}];
        self.picker_screens=true;self.source=0;self.share_picker=true;
    }
    pub fn preview(&mut self,channel:Channel,user:User,peers:Vec<User>){
        self.disconnect();self.channel=Some(channel);self.user=Some(user.clone());self.participants.insert(user.id.clone(),user);
        for user in peers.into_iter().take(3){self.participants.insert(user.id.clone(),user);}
        self.status="Offline call preview · no devices or network active".into();self.chat=false;self.ping_ms=Some(42);
    }
    pub fn show(&mut self, ctx: &egui::Context, _images: &mut crate::assets::Images) -> Vec<Value> {
        if self.share_picker {
            self.share_picker_modal(ctx);
        }
        if self.chat && self.watch_key.is_some() {
            self.picture_in_picture(ctx);
        }
        if self.fullscreen && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.toggle_fullscreen();
        }
        if let Some(error) = self.error.clone() {
            egui::Window::new("Call error")
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(error);
                    if ui.button("Dismiss").clicked() {
                        self.error = None;
                    }
                });
        }
        std::mem::take(&mut self.outbound)
    }
    /// Discord-style Screen Share dialog: Applications / Screens tabs, stream quality and Go Live.
    fn share_picker_modal(&mut self,ctx:&egui::Context){
        use egui::{Color32,RichText};
        let screen=|id:&voice::screen::SourceId|!matches!(id,voice::screen::SourceId::Window(_));
        let mut go=false;let mut cancel=false;
        let modal=egui::Modal::new(egui::Id::new("share-picker")).show(ctx,|ui|{
            ui.set_width(560.);
            ui.label(RichText::new("Screen Share").size(20.).strong());
            ui.label(RichText::new("Choose a window or a whole screen to stream to the call.").color(Color32::GRAY));
            ui.add_space(10.);
            ui.horizontal(|ui|{
                for (screens,label) in [(false,"Applications"),(true,"Screens")]{
                    if ui.selectable_label(self.picker_screens==screens,RichText::new(label).size(14.).strong()).clicked()&&self.picker_screens!=screens{
                        self.picker_screens=screens;
                        if let Some(first)=self.sources.iter().position(|s|screen(&s.id)==screens){self.source=first;}
                    }
                }
            });
            ui.separator();
            let shown:Vec<usize>=(0..self.sources.len()).filter(|i|screen(&self.sources[*i].id)==self.picker_screens).collect();
            egui::ScrollArea::vertical().id_salt("share-sources").max_height(300.).show(ui,|ui|{
                if shown.is_empty(){ui.label(RichText::new(if self.picker_screens{"No screens found."}else{"No open windows to share."}).color(Color32::GRAY));}
                let width=(ui.available_width()-10.)/2.;
                for pair in shown.chunks(2){ui.horizontal(|ui|{for index in pair{
                    let selected=self.source==*index;
                    let (rect,response)=ui.allocate_exact_size(Vec2::new(width,58.),egui::Sense::click());
                    let fill=if selected{Color32::from_gray(52)}else if response.hovered(){Color32::from_gray(44)}else{Color32::from_gray(36)};
                    ui.painter().rect_filled(rect,8,fill);
                    if selected{ui.painter().rect_stroke(rect,8,egui::Stroke::new(2.0_f32,Color32::from_rgb(88,101,242)),egui::StrokeKind::Inside);}
                    // A little monitor or window glyph.
                    let icon=egui::Rect::from_center_size(egui::pos2(rect.left()+28.,rect.center().y),Vec2::new(26.,18.));
                    let stroke=egui::Stroke::new(1.5_f32,Color32::from_gray(200));
                    ui.painter().rect_stroke(icon,3,stroke,egui::StrokeKind::Middle);
                    if screen(&self.sources[*index].id){ui.painter().line_segment([icon.center_bottom()+Vec2::new(-6.,5.),icon.center_bottom()+Vec2::new(6.,5.)],stroke);}
                    else{ui.painter().line_segment([icon.left_top()+Vec2::new(0.,5.),icon.right_top()+Vec2::new(0.,5.)],stroke);}
                    let name=ui.painter().layout(self.sources[*index].name.clone(),egui::FontId::proportional(13.),Color32::WHITE,rect.width()-62.);
                    ui.painter().galley(egui::pos2(rect.left()+52.,rect.center().y-name.size().y/2.),name,Color32::WHITE);
                    if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked(){self.source=*index;}
                }});}
            });
            ui.add_space(10.);
            ui.label(RichText::new("STREAM QUALITY").size(11.).strong().color(Color32::GRAY));
            ui.horizontal(|ui|{
                ui.label("Resolution");
                for height in [720,1080]{if ui.selectable_label(self.prefs.screen_height==height,format!("{height}p")).clicked(){self.prefs.screen_height=height;}}
                ui.add_space(16.);ui.label("Frame rate");
                for fps in [15,30,60]{if ui.selectable_label(self.prefs.screen_fps==fps,format!("{fps} FPS")).clicked(){self.prefs.screen_fps=fps;}}
            });
            ui.checkbox(&mut self.share_audio,"Share sound (other apps, not Eclipse)");
            ui.checkbox(&mut self.prefs.stream_preview,"Show a preview of my stream").on_hover_text("Uploads a small picture of your stream to Discord every minute, so people can see what you're sharing before they watch.");
            ui.add_space(12.);
            ui.horizontal(|ui|{
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                    let ready=shown.contains(&self.source);
                    if ui.add_enabled(ready,egui::Button::new(RichText::new("Go Live").strong().color(Color32::WHITE)).fill(Color32::from_rgb(88,101,242)).min_size(Vec2::new(96.,32.))).clicked(){go=true;}
                    if ui.add(egui::Button::new("Cancel").frame(false)).clicked(){cancel=true;}
                });
            });
        });
        if modal.should_close(){cancel=true;}
        if go{match self.start_share(){Ok(payload)=>{self.outbound.push(payload);self.share_picker=false;},Err(error)=>self.error=Some(error.into())}}
        else if cancel{self.share_picker=false;}
    }
    fn apply_controls(&self) {
        if let Some(media) = &self.media {
            let muted = self.muted || self.server_muted;
            let deafened = self.deafened || self.server_deafened;
            media.controls.send_modify(|c| {
                c.muted = muted || deafened;
                c.deafened = deafened;
                c.user_volumes=[(0,100);64];for (slot,(user,volume)) in c.user_volumes.iter_mut().zip(self.volumes.iter().take(64)){*slot=(*user,*volume);}
                c.stream_volume=self.stream_volume;
            });
            self.control_bits.store(muted as u8|(deafened as u8)<<1|(self.prefs.ui_sounds as u8)<<2,Ordering::Release);
            if let Ok(audio) = media.audio.lock() {
                audio.set_controls(muted, deafened);
                audio.set_hold(!hotkey_open(self.ptt.load(Ordering::Acquire),key_down));
            }
        }
    }
    fn update_controls(&mut self) {
        self.apply_controls();
        if self.channel.is_some(){self.outbound.push(voice_payload(
            self.channel.as_ref(),
            self.muted,
            self.deafened,
            self.video,
        ));}
    }
    pub fn active(&self)->bool{self.channel.is_some()}
    pub fn channel(&self)->Option<&Channel>{self.channel.as_ref()}
    /// The call's live ping in milliseconds, once the voice server has answered a heartbeat.
    pub fn ping(&self)->Option<u32>{self.ping_ms.filter(|_|self.active())}
    pub fn connection_label(&self)->&str{if self.ready{"Voice connected"}else{"Connecting voice…"}}
    pub fn self_speaking(&self)->bool{
        // Keep checking the push-to-talk key so the green ring follows it even while Eclipse is idle.
        if self.prefs.push_to_talk&&self.active(){self.ctx.request_repaint_after(Duration::from_millis(50));}
        self.self_speaking_with(key_down)
    }
    fn self_speaking_with(&self,down:impl Fn(i32)->bool)->bool{
        if !(self.active()&&self.ready&&!self.muted&&!self.deafened&&!self.server_muted&&!self.server_deafened&&hotkey_open(self.ptt.load(Ordering::Acquire),&down)){return false;}
        // With push to talk, holding the key means you're talking, like Discord; otherwise the microphone decides.
        self.prefs.push_to_talk||self.user.as_ref().and_then(|u|u.id.parse::<u64>().ok()).is_some_and(|id|self.speaking.contains(&id))
    }
    /// Keeps the call screen's tiles in step with who Discord says is in the channel, the same list
    /// the sidebar shows, so people who were already there when you joined appear too.
    /// Keeps the call screen in step with who Discord says is in the channel (the same list the
    /// sidebar shows), including people who were already there, or already live, when you joined.
    pub fn sync_roster(&mut self,users:Vec<(User,bool)>){
        let Some(channel)=self.channel.clone() else{return};
        let me=self.user.as_ref().map(|u|u.id.clone()).unwrap_or_default();
        if !users.is_empty(){
            if channel.guild_id.is_some(){self.participants.retain(|id,_|*id==me||users.iter().any(|(u,_)|&u.id==id));}
            self.streams.retain(|(_,id)|users.iter().any(|(u,live)|&u.id==id&&*live));
        }
        for (user,live) in users.into_iter().filter(|(u,_)|!u.id.is_empty()).take(64){
            if live&&user.id!=me&&!self.streams.iter().any(|(_,id)|*id==user.id){self.streams.push((stream_key(&channel,&user.id),user.id.clone()));}
            let entry=self.participants.entry(user.id.clone()).or_insert_with(||user.clone());
            if entry.username.is_empty(){*entry=user;}
        }
        if let Some(user)=self.pending_watch.clone(){
            if self.ready&&self.is_streaming(&user){let _=self.watch(&user);}
        }
    }
    /// Whether someone in this call is talking right now: you by your microphone or
    /// push-to-talk key, others by the voice server's speaking reports.
    pub fn is_speaking(&self,user:&str)->bool{
        if self.user.as_ref().is_some_and(|u|u.id==user){return self.self_speaking();}
        self.ready&&user.parse::<u64>().is_ok_and(|id|self.speaking.contains(&id))
    }
    /// A preview picture of your stream ready to send to Discord: (stream key, data URL).
    pub fn take_preview_upload(&mut self)->Option<(String,String)>{self.upload.take()}
    /// Choices made in the Screen Share dialog, so the app can remember them.
    pub fn share_choices(&self)->(u32,u32,bool){(self.prefs.screen_height,self.prefs.screen_fps,self.prefs.stream_preview)}
    pub fn is_streaming(&self,user:&str)->bool{self.streams.iter().any(|(_,id)|id==user)}
    pub fn watching(&self)->Option<&str>{let key=self.watch_key.as_deref()?;self.streams.iter().find(|(k,_)|k==key).map(|(_,id)|id.as_str())}
    /// Watch Stream from outside the call: starts once the call has connected.
    pub fn watch_when_ready(&mut self,user:&str){self.pending_watch=Some(user.to_owned());self.chat=false;}
    /// Starts watching someone's screen share in this call, replacing any stream being watched.
    pub fn watch(&mut self,user:&str)->Result<(),&'static str>{
        let key=self.streams.iter().find(|(_,id)|id==user).map(|(key,_)|key.clone()).ok_or("They are no longer streaming.")?;
        if !self.ready||self.media.is_none(){self.pending_watch=Some(user.to_owned());return Ok(());}
        self.pending_watch=None;self.chat=false;
        if self.watch_key.as_deref()==Some(key.as_str()){return Ok(());}
        self.stop_watching();
        if let Some(media)=&mut self.media{media.watch_conn=StreamConn::default();}
        self.watch_key=Some(key.clone());
        self.outbound.push(json!({"op":20,"d":{"stream_key":key}}));
        Ok(())
    }
    pub fn stop_watching(&mut self){
        let Some(key)=self.watch_key.take() else{return};
        // Tell Discord we left; otherwise it still counts us as watching and never answers the
        // next Watch Stream, which then hangs on "Connecting to stream…".
        self.outbound.push(json!({"op":19,"d":{"stream_key":key}}));
        self.left_stream=Some((key,Instant::now()));
        if let Some(media)=&mut self.media{if let Some(task)=media.watch_task.take(){task.abort();}}
        self.textures.clear();
        self.expanded=false;
    }
    pub fn muted(&self)->bool{self.muted}
    pub fn deafened(&self)->bool{self.deafened}
    pub fn toggle_mute(&mut self){self.muted=!self.muted;self.update_controls();crate::sounds::play(if self.muted{crate::sounds::Cue::Mute}else{crate::sounds::Cue::Unmute},self.prefs.ui_sounds);}
    pub fn toggle_deafen(&mut self){self.deafened=!self.deafened;self.update_controls();crate::sounds::play(if self.deafened{crate::sounds::Cue::Deafen}else{crate::sounds::Cue::Undeafen},self.prefs.ui_sounds);}
    pub fn leave(&mut self){if self.active(){self.hang_up();}}
    pub fn volume(&self,user:&str)->u16{user.parse::<u64>().ok().and_then(|id|self.volumes.get(&id).copied()).unwrap_or(100)}
    pub fn set_volume(&mut self,user:&str,value:u16){if let Ok(id)=user.parse(){if self.volumes.len()<64||self.volumes.contains_key(&id){self.volumes.insert(id,value.min(if self.prefs.volume_booster{1000}else{200}));self.apply_controls();}}}
    pub fn configure(&mut self,prefs:&crate::preferences::Preferences){
        self.prefs=prefs.clone();if !prefs.volume_booster{for value in self.volumes.values_mut(){*value=(*value).min(200);}self.stream_volume=self.stream_volume.min(200);}self.ptt.store((prefs.ptt_key as u64&255)|((prefs.ptt_ctrl as u64)<<8)|((prefs.ptt_shift as u64)<<9)|((prefs.ptt_alt as u64)<<10)|((prefs.push_to_talk as u64)<<16),Ordering::Release);self.configure_audio();self.apply_controls();
    }
    fn configure_audio(&self){if let Some(media)=&self.media{if let Ok(audio)=media.audio.lock(){audio.set_input_enabled(true);audio.set_devices(voice::audio::Devices{input:self.prefs.input.clone(),output:self.prefs.output.clone()});audio.set_gain(self.prefs.input_gain,self.prefs.output_gain);audio.set_processing(effective_processing(&self.prefs));}}}
}
fn effective_processing(prefs:&crate::preferences::Preferences)->voice_model::voice_settings::Processing {
    let mut processing=prefs.processing.effective();if prefs.push_to_talk{processing.sensitivity_db=None;}processing
}
fn key_down(key:i32)->bool{unsafe{windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(key)<0}}
fn hotkey_open(config:u64,down:impl Fn(i32)->bool)->bool{
    if config&(1<<16)==0{return true;}
    down((config&255)as i32)&&((config&(1<<8)==0)||down(0x11))&&((config&(1<<9)==0)||down(0x10))&&((config&(1<<10)==0)||down(0x12))
}
/// Discord-style Watch Stream pill: a rounded blurple button with a small screen glyph.
pub fn watch_pill(ui:&egui::Ui,rect:egui::Rect,id:egui::Id,label:&str)->bool{
    let response=ui.interact(rect,id,egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let fill=if response.hovered(){egui::Color32::from_rgb(71,82,196)}else{egui::Color32::from_rgb(88,101,242)};
    ui.painter().rect_filled(rect,rect.height()/2.,fill);
    let text=ui.painter().layout_no_wrap(label.into(),egui::FontId::proportional((rect.height()*0.45).clamp(10.,13.)),egui::Color32::WHITE);
    let glyph=Vec2::new(11.,8.);let total=glyph.x+5.+text.size().x;
    let left=rect.center().x-total/2.;
    let screen=egui::Rect::from_min_size(egui::pos2(left,rect.center().y-glyph.y/2.-1.),glyph);
    let stroke=egui::Stroke::new(1.3_f32,egui::Color32::WHITE);
    ui.painter().rect_stroke(screen,1.5,stroke,egui::StrokeKind::Middle);
    ui.painter().line_segment([screen.center_bottom()+Vec2::new(-3.,2.5),screen.center_bottom()+Vec2::new(3.,2.5)],stroke);
    ui.painter().galley(egui::pos2(left+glyph.x+5.,rect.center().y-text.size().y/2.),text,egui::Color32::WHITE);
    response.clicked()
}
/// A round icon button painted over the stream.
fn icon_button(ui:&egui::Ui,rect:egui::Rect,id:egui::Id,tip:&str,draw:impl Fn(&egui::Painter,egui::Pos2,egui::Stroke))->bool{
    let response=ui.interact(rect,id,egui::Sense::click()).on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand);
    ui.painter().rect_filled(rect,8,egui::Color32::from_black_alpha(if response.hovered(){200}else{140}));
    draw(ui.painter(),rect.center(),egui::Stroke::new(1.6_f32,egui::Color32::WHITE));
    response.clicked()
}
/// A button painted over a tile without taking part in the tile layout.
fn overlay_button(ui:&egui::Ui,rect:egui::Rect,id:egui::Id,label:&str,fill:egui::Color32)->bool{
    let response=ui.interact(rect,id,egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let fill=if response.hovered(){fill.gamma_multiply(0.85)}else{fill};
    ui.painter().rect_filled(rect,6,fill);
    ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,label,egui::FontId::proportional(14.),egui::Color32::WHITE);
    response.clicked()
}
/// Screen shares skip the 640x360 camera path: each decoded picture goes to the screen at its
/// own resolution (only pictures over 1080p are scaled down), replacing any frame not yet shown.
fn stream_sink(latest:Arc<Mutex<Option<egui::ColorImage>>>,active:Arc<AtomicU64>,epoch:u64,repaint:egui::Context)->voice::VideoSink{
    Arc::new(move|frame|{
        let (width,height)=(frame.width as usize,frame.height as usize);
        if active.load(Ordering::Acquire)!=epoch||width==0||height==0||frame.rgba.len()!=width*height*4{return;}
        let image=if width<=1920&&height<=1080{egui::ColorImage::from_rgba_premultiplied([width,height],frame.rgba)}
            else if let Some(image)=image::RgbaImage::from_raw(frame.width,frame.height,frame.rgba.to_vec()){
                let image=image::DynamicImage::ImageRgba8(image).resize(1920,1080,image::imageops::FilterType::Triangle).to_rgba8();
                egui::ColorImage::from_rgba_premultiplied([image.width() as usize,image.height() as usize],image.as_raw())
            }else{return};
        if let Ok(mut latest)=latest.lock(){*latest=Some(image);}
        repaint.request_repaint();
    })
}
/// A small JPEG data URL of a stream picture, the form Discord takes for stream previews.
fn preview_jpeg(image:&image::RgbaImage)->Option<String>{
    use base64::{engine::general_purpose::STANDARD,Engine};
    let small=image::DynamicImage::ImageRgba8(image.clone()).thumbnail(512,288).to_rgb8();
    let mut bytes=std::io::Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes,75).encode_image(&small).ok()?;
    Some(format!("data:image/jpeg;base64,{}",STANDARD.encode(bytes.into_inner())))
}
/// Lays out `count` tiles of one size in as many full rows as fit, each row centered, and
/// calls `tile` with each tile's index and place.
fn centered_rows(ui:&mut egui::Ui,count:usize,size:Vec2,gap:f32,mut tile:impl FnMut(&mut egui::Ui,usize,egui::Rect)){
    let width=ui.available_width();
    let per_row=(((width+gap)/(size.x+gap)).floor() as usize).max(1);
    let mut index=0;
    while index<count{
        let in_row=per_row.min(count-index);
        let row_width=in_row as f32*size.x+(in_row-1) as f32*gap;
        ui.horizontal(|ui|{
            ui.spacing_mut().item_spacing.x=gap;
            ui.add_space(((width-row_width)/2.).max(0.));
            for i in index..index+in_row{
                let (rect,_)=ui.allocate_exact_size(size,egui::Sense::hover());
                tile(ui,i,rect);
            }
        });
        ui.add_space(gap);
        index+=in_row;
    }
}
/// Discord's red LIVE pill.
fn live_badge(ui:&egui::Ui,at:egui::Pos2){
    let galley=ui.painter().layout_no_wrap("LIVE".into(),egui::FontId::proportional(11.),egui::Color32::WHITE);
    let rect=egui::Rect::from_min_size(at,galley.size()+Vec2::new(10.,4.));
    ui.painter().rect_filled(rect,4,egui::Color32::from_rgb(218,55,60));ui.painter().galley(rect.center()-galley.size()/2.,galley,egui::Color32::WHITE);
}
pub fn stream_key(channel: &Channel, user: &str) -> String {
    match &channel.guild_id {
        Some(guild) => format!("guild:{guild}:{}:{user}", channel.id),
        None => format!("call:{}:{user}", channel.id),
    }
}
fn audio_gate_after(
    audio: &Arc<Mutex<voice::audio::Audio>>,
) -> Result<std::sync::MutexGuard<'_, voice::audio::Audio>, ()> {
    audio.lock().map_err(|_| ())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn successful_voice_connection_and_leave_play_once_and_respect_sounds_setting(){
        crate::sounds::take_played();let mut calls=Calls::new(egui::Context::default());
        calls.join(&Channel{id:"12".into(),..Default::default()},&User{id:"56".into(),..Default::default()},false).unwrap();assert!(crate::sounds::take_played().is_empty());
        calls.tx.try_send((calls.epoch,Notice::State(Status::WaitingForPeer))).unwrap();calls.poll();calls.tx.try_send((calls.epoch,Notice::State(Status::Ready{privacy_code:String::new()}))).unwrap();calls.poll();
        calls.disconnect();calls.disconnect();assert_eq!(crate::sounds::take_played(),vec![crate::sounds::Cue::Join,crate::sounds::Cue::Leave]);
        calls.prefs.ui_sounds=false;calls.tx.try_send((calls.epoch,Notice::State(Status::WaitingForPeer))).unwrap();calls.poll();calls.disconnect();assert!(crate::sounds::take_played().is_empty());
    }
    #[test]fn modifier_only_bindings_open_and_release_the_microphone_gate(){let enabled=1u64<<16;for key in [16,17,18]{assert!(hotkey_open(enabled|key as u64,|k|k==key));assert!(!hotkey_open(enabled|key as u64,|_|false));}let ctrl_shift=enabled|16|(1<<8);assert!(hotkey_open(ctrl_shift,|k|k==16||k==17));assert!(!hotkey_open(ctrl_shift,|k|k==16));}
    #[test]
    fn push_to_talk_releases_on_key_or_modifier_release(){
        let f8=119u64;let enabled=1u64<<16;let chord=f8|enabled|(1<<8)|(1<<9);
        assert!(hotkey_open(0,|_|false));
        assert!(!hotkey_open(f8|enabled,|_|false));
        assert!(hotkey_open(f8|enabled,|key|key==119));
        assert!(!hotkey_open(chord,|key|key==119||key==17));
        assert!(hotkey_open(chord,|key|[119,17,16].contains(&key)));
        assert!(!hotkey_open(chord,|key|[17,16].contains(&key)));
        assert!(hotkey_open(enabled|5,|key|key==5));
    }
    #[test]
    fn changing_the_hotkey_replaces_the_active_gate(){
        let mut calls=Calls::new(egui::Context::default());
        let mut prefs=crate::preferences::Preferences{push_to_talk:true,..Default::default()};calls.configure(&prefs);
        assert!(hotkey_open(calls.ptt.load(Ordering::Acquire),|k|k==119));
        prefs.ptt_key=5;prefs.ptt_alt=true;calls.configure(&prefs);
        let config=calls.ptt.load(Ordering::Acquire);
        assert!(!hotkey_open(config,|k|k==119));assert!(!hotkey_open(config,|k|k==5));assert!(hotkey_open(config,|k|k==5||k==18));
        let restored:crate::preferences::Preferences=serde_json::from_slice(&serde_json::to_vec(&prefs).unwrap()).unwrap();assert_eq!(restored.ptt_key,5);assert!(restored.ptt_alt&&restored.push_to_talk);
        assert!(effective_processing(&prefs).sensitivity_db.is_none());prefs.push_to_talk=false;assert_eq!(effective_processing(&prefs).sensitivity_db,Some(-55));
    }
    #[test]
    fn pre_call_mute_and_boosted_volume_are_preserved_and_bounded(){
        let mut calls=Calls::new(egui::Context::default());calls.toggle_mute();assert!(calls.outbound.is_empty());
        calls.join(&Channel{id:"12".into(),..Default::default()},&User{id:"56".into(),..Default::default()},false).unwrap();assert!(calls.muted());
        calls.set_volume("78",1500);assert_eq!(calls.volume("78"),1000);
        let prefs=crate::preferences::Preferences{volume_booster:false,..Default::default()};calls.configure(&prefs);assert_eq!(calls.volume("78"),200);
    }
    #[test]
    fn join_and_leave_signal_scope() {
        let channel = Channel {
            id: "12".into(),
            guild_id: Some("34".into()),
            ..Default::default()
        };
        let mut calls = Calls::new(egui::Context::default());
        let user = User {
            id: "56".into(),
            ..Default::default()
        };
        let join = calls.join(&channel, &user, false).unwrap();
        assert_eq!(join["d"]["channel_id"], "12");
        assert_eq!(join["d"]["guild_id"], "34");
        assert!(calls.media.is_none());
        calls.hang_up();
        let leave = calls.outbound.pop().unwrap();
        assert!(leave["d"]["channel_id"].is_null());
        assert_eq!(leave["d"]["guild_id"], "34");
        assert!(calls.media.is_none());
        assert_eq!(stream_key(&channel, "56"), "guild:34:12:56");
    }
    #[test]
    fn old_callbacks_and_unrelated_servers_cannot_change_new_call() {
        let mut calls = Calls::new(egui::Context::default());
        let user = User {
            id: "56".into(),
            ..Default::default()
        };
        let mut channel = Channel {
            id: "12".into(),
            guild_id: Some("34".into()),
            ..Default::default()
        };
        calls.join(&channel, &user, false).unwrap();
        let old = calls.epoch;
        calls.disconnect();
        channel.id = "13".into();
        calls.join(&channel, &user, false).unwrap();
        calls
            .tx
            .try_send((old, Notice::Error("stale callback")))
            .unwrap();
        calls.poll();
        assert_eq!(calls.channel.as_ref().unwrap().id, "13");
        calls.signal(
            "VOICE_SERVER_UPDATE",
            &json!({"guild_id":"99","token":"unrelated","endpoint":"example.com"}),
        );
        assert!(calls.token.is_none());
        assert!(calls.media.is_none());
        calls.signal("VOICE_STATE_UPDATE",&json!({"user_id":"56","channel_id":"13","session_id":"local-test","mute":true,"deaf":true}));
        assert!(calls.server_muted && calls.server_deafened);
        assert!(calls.media.is_none());
        let (intent, epoch) = calls.intent();
        calls.hang_up();
        assert_ne!(intent.load(Ordering::Acquire), epoch);
    }
    #[test]
    fn watching_again_after_stopping_reconnects(){
        let mut calls=Calls::new(egui::Context::default());
        let me=User{id:"1".into(),username:"me".into(),..Default::default()};
        let channel=Channel{id:"10".into(),guild_id:Some("5".into()),kind:2,..Default::default()};
        calls.join(&channel,&me,false).unwrap();
        calls.sync_roster(vec![(me.clone(),false),(User{id:"2".into(),username:"friend".into(),..Default::default()},true)]);
        let key=stream_key(&channel,"2");
        // Pretend the call connected: watching sends the watch request at once.
        calls.ready=true;calls.watch_key=Some(key.clone());calls.outbound.clear();
        calls.stop_watching();
        assert_eq!(calls.outbound.pop(),Some(json!({"op":19,"d":{"stream_key":key}})),"leaving is sent to Discord");
        calls.watch_key=Some(key.clone());
        calls.signal("STREAM_DELETE",&json!({"stream_key":key}));
        assert_eq!(calls.watch_key.as_deref(),Some(key.as_str()),"the reply to leaving does not cancel watching again");
        calls.signal("STREAM_DELETE",&json!({"stream_key":key}));
        assert!(calls.watch_key.is_none(),"a real end of the stream still stops it");
    }
    #[test]
    fn stream_preview_pictures_are_small_jpeg_data_urls(){
        let image=image::RgbaImage::from_pixel(960,540,image::Rgba([10,200,30,255]));
        let url=preview_jpeg(&image).unwrap();
        assert!(url.starts_with("data:image/jpeg;base64,")&&url.len()<64*1024,"{} bytes",url.len());
    }
    #[test]
    fn newcomers_appear_on_the_right_and_leavers_close_the_gap(){
        let mut calls=Calls::new(egui::Context::default());
        let person=|id:&str|User{id:id.into(),username:id.into(),..Default::default()};
        for id in ["30","10"]{calls.participants.insert(id.into(),person(id));}
        assert_eq!(calls.in_join_order().iter().map(|u|u.id.as_str()).collect::<Vec<_>>(),["10","30"]);
        calls.participants.insert("20".into(),person("20"));
        assert_eq!(calls.in_join_order().iter().map(|u|u.id.as_str()).collect::<Vec<_>>(),["10","30","20"],"a newcomer goes on the right, not by id");
        calls.participants.remove("30");
        assert_eq!(calls.in_join_order().iter().map(|u|u.id.as_str()).collect::<Vec<_>>(),["10","20"]);
    }
    #[test]
    fn being_moved_follows_to_the_new_channel_instead_of_hanging_up(){
        let mut calls=Calls::new(egui::Context::default());
        let me=User{id:"1".into(),username:"me".into(),..Default::default()};
        calls.join(&Channel{id:"10".into(),guild_id:Some("5".into()),kind:2,name:Some("Join to Create".into()),..Default::default()},&me,false).unwrap();
        calls.signal("VOICE_STATE_UPDATE",&json!({"user_id":"1","channel_id":"10","session_id":"s1"}));
        assert!(calls.active());
        // The bot moves us into the channel it just created.
        calls.signal("VOICE_STATE_UPDATE",&json!({"user_id":"1","channel_id":"11","session_id":"s1"}));
        assert!(calls.active(),"still in a call");
        assert_eq!(calls.channel().map(|c|c.id.as_str()),Some("11"));
        assert_eq!(calls.moved_channel(),Some("11"));
        calls.set_moved_channel(Channel{id:"11".into(),guild_id:Some("5".into()),kind:2,name:Some("Jason's channel".into()),..Default::default()});
        assert!(calls.moved_channel().is_none()&&calls.channel().is_some_and(|c|c.name.as_deref()==Some("Jason's channel")));
        // Being disconnected still ends the call.
        calls.signal("VOICE_STATE_UPDATE",&json!({"user_id":"1","channel_id":null,"session_id":"s1"}));
        assert!(!calls.active());
    }
    #[test]
    fn friends_show_as_speaking_while_the_voice_server_reports_them(){
        let mut calls=Calls::new(egui::Context::default());
        calls.user=Some(User{id:"56".into(),..Default::default()});
        calls.channel=Some(Channel{id:"12".into(),..Default::default()});
        calls.speaking=vec![77];
        assert!(!calls.is_speaking("77"),"not until the call is connected");
        calls.ready=true;
        assert!(calls.is_speaking("77")&&!calls.is_speaking("78")&&!calls.is_speaking("not-a-number"));
        calls.speaking.clear();assert!(!calls.is_speaking("77"));
    }
    #[test]
    fn own_speaking_indicator_requires_connected_unmuted_transport_activity() {
        let mut calls=Calls::new(egui::Context::default());
        calls.user=Some(User{id:"56".into(),..Default::default()});
        calls.channel=Some(Channel{id:"12".into(),..Default::default()});
        calls.ptt.store(0,Ordering::Release);calls.speaking=vec![56];
        assert!(!calls.self_speaking());calls.ready=true;assert!(calls.self_speaking());
        calls.muted=true;assert!(!calls.self_speaking());calls.muted=false;
        calls.server_muted=true;assert!(!calls.self_speaking());calls.server_muted=false;
        calls.deafened=true;assert!(!calls.self_speaking());calls.deafened=false;
        calls.server_deafened=true;assert!(!calls.self_speaking());calls.server_deafened=false;
        calls.speaking=vec![99];assert!(!calls.self_speaking());
        calls.speaking=vec![56];calls.channel=None;assert!(!calls.self_speaking());
    }
    #[test]
    fn watched_streams_keep_their_resolution_up_to_1080p(){
        let latest=Arc::new(Mutex::new(None));let active=Arc::new(AtomicU64::new(7));
        let sink=stream_sink(latest.clone(),active.clone(),7,egui::Context::default());
        let frame=|w:u32,h:u32|vec![255u8;(w*h*4) as usize];
        let pixels=frame(1280,720);sink(voice::RemoteFrame{user:1,width:1280,height:720,rgba:&pixels});
        assert_eq!(latest.lock().unwrap().take().map(|i:egui::ColorImage|i.size),Some([1280,720]),"720p is shown at 720p, not shrunk");
        let pixels=frame(2560,1440);sink(voice::RemoteFrame{user:1,width:2560,height:1440,rgba:&pixels});
        assert_eq!(latest.lock().unwrap().take().map(|i:egui::ColorImage|i.size),Some([1920,1080]));
        active.store(8,Ordering::Release);let pixels=frame(4,4);sink(voice::RemoteFrame{user:1,width:4,height:4,rgba:&pixels});
        assert!(latest.lock().unwrap().is_none(),"frames from an old call are dropped");
    }
    #[test]
    fn push_to_talk_ring_follows_the_key_not_the_microphone(){
        let mut calls=Calls::new(egui::Context::default());
        calls.user=Some(User{id:"56".into(),..Default::default()});
        calls.channel=Some(Channel{id:"12".into(),..Default::default()});calls.ready=true;
        calls.configure(&crate::preferences::Preferences{push_to_talk:true,ptt_key:119,..Default::default()});
        calls.speaking=vec![];
        assert!(calls.self_speaking_with(|k|k==119),"holding the key without mic activity should show the ring");
        calls.speaking=vec![56];
        assert!(!calls.self_speaking_with(|_|false),"mic activity without the key should not show the ring");
        calls.muted=true;assert!(!calls.self_speaking_with(|k|k==119));
    }
    #[test]
    fn call_tiles_include_people_already_in_the_channel(){
        let mut calls=Calls::new(egui::Context::default());
        let me=User{id:"1".into(),username:"me".into(),..Default::default()};
        calls.join(&Channel{id:"10".into(),guild_id:Some("5".into()),kind:2,..Default::default()},&me,false).unwrap();
        let friend=|id:&str,name:&str|User{id:id.into(),username:name.into(),..Default::default()};
        calls.sync_roster(vec![(me.clone(),false),(friend("2","husain"),true),(friend("3","mahin"),false)]);
        assert_eq!(calls.participants.len(),3);
        // husain was already live when we joined: the stream is offered.
        assert!(calls.is_streaming("2")&&!calls.is_streaming("3"));
        calls.sync_roster(vec![(me.clone(),false),(friend("2","husain"),false)]);
        assert!(calls.participants.contains_key("1")&&calls.participants.contains_key("2")&&!calls.participants.contains_key("3"));
        assert!(!calls.is_streaming("2"),"stopping the stream removes it");
        // Watching before the call is connected waits for the connection.
        calls.sync_roster(vec![(me.clone(),false),(friend("2","husain"),true)]);
        calls.watch("2").unwrap();assert_eq!(calls.pending_watch.as_deref(),Some("2"));assert!(calls.watching().is_none());
    }
}
