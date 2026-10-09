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
    stream_server: Option<Id>,
    stream_channel: Option<Id>,
    stream_token: Option<Zeroizing<String>>,
    stream_endpoint: Option<String>,
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
    speaking: Vec<u64>,
    frames: Arc<Mutex<HashMap<u64, VideoFrame>>>,
    textures: HashMap<u64, egui::TextureHandle>,
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
        self.speaking.clear();
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
                        if self.session_id.is_some() {
                            self.disconnect();
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
                    if let Some(media) = &mut self.media {
                        if kind == "STREAM_CREATE" {
                            media.stream_server =
                                data["rtc_server_id"].as_str().and_then(|s| id(s).ok());
                            media.stream_channel =
                                data["rtc_channel_id"].as_str().and_then(|s| id(s).ok());
                        } else {
                            media.stream_token =
                                data["token"].as_str().map(|s| Zeroizing::new(s.into()));
                            media.stream_endpoint = data["endpoint"].as_str().map(str::to_owned);
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
                if key == self.watch_key.as_deref() {
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
        if let Ok(audio)=audio.lock(){audio.set_input_enabled(true);audio.set_controls(self.muted||self.server_muted||!hotkey_open(self.ptt.load(Ordering::Acquire),key_down),self.deafened||self.server_deafened);audio.set_gain(self.prefs.input_gain,self.prefs.output_gain);audio.set_processing(effective_processing(&self.prefs));}
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
            stream_server: None,
            stream_channel: None,
            stream_token: None,
            stream_endpoint: None,
        });
        if let Some(media)=&self.media {
            let weak=Arc::downgrade(&media.audio);let stop=media.ptt_stop.clone();let config=self.ptt.clone();let bits=self.control_bits.clone();
            // Push to talk gates the always-open microphone like mute does. Toggling the input
            // device itself reopened the audio streams on every press, delaying speech.
            std::thread::spawn(move||{let mut last=None;while !stop.load(Ordering::Acquire){
                let Some(shared)=weak.upgrade()else{return;};
                let settings=config.load(Ordering::Acquire);let open=hotkey_open(settings,key_down);let state=bits.load(Ordering::Acquire);let(muted,deafened)=(state&1!=0,state&2!=0);
                if let Ok(audio)=shared.lock(){audio.set_controls(muted||!open,deafened);}drop(shared);
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
            media.stream_server = None;
            media.stream_channel = None;
            media.stream_token = None;
            media.stream_endpoint = None;
        }
        self.stream_key = None;
    }
    fn start_share(&mut self) -> Result<Value, &'static str> {
        if self.watch_key.is_some() {
            return Err("Stop watching the other stream before sharing your screen.");
        }
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
        media.stream_server = None;
        media.stream_channel = None;
        media.stream_token = None;
        media.stream_endpoint = None;
        let channel = self.channel.as_ref().ok_or("Call ended")?;
        let user = self.user.as_ref().ok_or("Call ended")?;
        self.stream_key = Some(stream_key(channel, &user.id));
        Ok(
            json!({"op":18,"d":{"type":if channel.guild_id.is_some(){"guild"}else{"call"},"guild_id":channel.guild_id,"channel_id":channel.id,"preferred_region":null}}),
        )
    }
    fn start_stream(&mut self) {
        let remote_sink = self.valid_sink();
        let Some(media) = &mut self.media else { return };
        let (Some(server), Some(channel), Some(token), Some(endpoint), Some(session), Some(user)) = (
            media.stream_server,
            media.stream_channel,
            media.stream_token.as_ref(),
            media.stream_endpoint.as_ref(),
            self.session_id.as_ref(),
            self.user.as_ref(),
        ) else {
            return;
        };
        let Ok(credentials) = (|| {
            Ok::<_, &'static str>(VoiceConnection {
                channel,
                guild: Some(server),
                user: id(&user.id)?,
                peer: None,
                session: Secret::new(session.to_string())?,
                token: Secret::new(token.to_string())?,
                endpoint: endpoint.clone(),
                request: 1,
            })
        })() else {
            return;
        };
        let identity = media.identity.clone();
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        let epoch = self.epoch;
        if let Some(video) = media.screen_video.take() {
            media.screen_task = Some(media.runtime.spawn(async move {
                let result = voice::run_stream(credentials, identity, video, move |_| {
                    ctx.request_repaint();
                    Ok(())
                })
                .await;
                let _ = tx.try_send((epoch, Notice::StreamEnded(false, result)));
            }));
        } else if self.watch_key.is_some() && media.watch_task.is_none() {
            let sink = remote_sink;
            let playback = media.stream_playback.clone();
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
            egui::ScrollArea::vertical().id_salt("call-tiles").max_height(tiles_height).show(ui,|ui|{
                let mut participants:Vec<_>=self.participants.values().cloned().collect();participants.sort_by(|a,b|a.id.cmp(&b.id));
                let width=((ui.available_width()-12.)/2.).max(150.);
                for row in participants.chunks(2){ui.horizontal_top(|ui|{for user in row {
                    let (rect,_)=ui.allocate_exact_size(Vec2::new(width,(width*0.5625).clamp(120.,280.)),egui::Sense::hover());let id=user.id.parse::<u64>().unwrap_or(0);
                    ui.painter().rect_filled(rect,8,egui::Color32::from_gray(39));
                    if let Some(texture)=self.textures.get(&id){egui::Image::new(texture).corner_radius(8).paint_at(ui,rect);}else{
                        let avatar=egui::Rect::from_center_size(rect.center(),Vec2::splat(76.));ui.painter().circle_filled(avatar.center(),38.,egui::Color32::from_gray(65));
                        if let Some(texture)=crate::assets::avatar_url(user,None,None).and_then(|url|images.texture(&url,avatar,ui.ctx())){egui::Image::new((texture,avatar.size())).corner_radius(38).paint_at(ui,avatar);}else{ui.painter().text(avatar.center(),egui::Align2::CENTER_CENTER,user.name().chars().take(2).collect::<String>(),egui::FontId::proportional(26.),egui::Color32::WHITE);}
                    }
                    let speaking=if self.user.as_ref().is_some_and(|u|u.id==user.id){self.self_speaking()}else{self.speaking.contains(&id)};
                    if speaking{ui.painter().rect_stroke(rect,8,egui::Stroke::new(2.0_f32,egui::Color32::from_rgb(35,165,90)),egui::StrokeKind::Inside);}
                    ui.painter().text(rect.left_bottom()+Vec2::new(12.,-12.),egui::Align2::LEFT_BOTTOM,user.name(),egui::FontId::proportional(14.),egui::Color32::WHITE);
                }});ui.add_space(12.);}
                for (id,texture) in &self.textures{if !self.participants.contains_key(&id.to_string()){ui.add(egui::Image::new(texture).max_width(ui.available_width()));}}
                for (key,user) in self.streams.clone(){ui.horizontal(|ui|{ui.label(format!("Screen share · {user}"));if self.watch_key.as_deref()==Some(&key){if ui.button("Stop watching").clicked(){if let Some(media)=&mut self.media{if let Some(task)=media.watch_task.take(){task.abort();}}self.watch_key=None;self.textures.clear();}}else if ui.add_enabled(self.stream_key.is_none()&&self.watch_key.is_none(),egui::Button::new("Watch stream")).clicked(){if let Some(media)=&mut self.media{media.stream_server=None;media.stream_channel=None;media.stream_token=None;media.stream_endpoint=None;}self.watch_key=Some(key.clone());self.outbound.push(json!({"op":20,"d":{"stream_key":key}}));}});}
                if self.watch_key.is_some(){let max=if self.prefs.volume_booster{1000}else{200};if ui.add(egui::Slider::new(&mut self.stream_volume,0..=max).text("Stream volume %")).changed(){self.apply_controls();}}
            });
            ui.add_space((ui.available_height()-96.).max(8.));
            ui.horizontal(|ui|{
                ui.add_space(((ui.available_width()-320.)/2.).max(0.));
                if control(ui,Control::Mic,self.muted,48.,"Mute / unmute").clicked(){self.toggle_mute();}
                if control(ui,Control::Headphones,self.deafened,48.,"Deafen / undeafen").clicked(){self.toggle_deafen();}
                if control(ui,Control::Camera,self.video,48.,"Camera on / off").clicked(){if let Err(e)=self.toggle_camera(!self.video){self.error=Some(e.into());}else{self.update_controls();}}
                if control(ui,Control::Screen,self.stream_key.is_some(),48.,"Share your screen").clicked(){if self.stream_key.is_some(){if let Some(key)=self.stream_key.clone(){self.outbound.push(json!({"op":19,"d":{"stream_key":key}}));}self.stop_share();}else if self.ready{match voice::screen::sources(){Ok(sources)=>{self.sources=sources;self.source=0;self.share_picker=true;},Err(e)=>self.error=Some(e.into())}}else{self.error=Some("Wait for the call to connect before sharing.".into());}}
                if control(ui,Control::Hangup,false,48.,"Disconnect").clicked(){self.hang_up();}
            });
            ui.add_space(8.);ui.vertical_centered(|ui|{if self.prefs.push_to_talk{ui.small(if hotkey_open(self.ptt.load(Ordering::Acquire),key_down){"Push to talk · microphone open"}else{"Push to talk · hold your hotkey to speak"});}ui.weak("Native encrypted media · live Discord interoperability remains unverified");});
        });ui.ctx().request_repaint_after(Duration::from_millis(33));
    }
    pub fn preview(&mut self,channel:Channel,user:User,peers:Vec<User>){
        self.disconnect();self.channel=Some(channel);self.user=Some(user.clone());self.participants.insert(user.id.clone(),user);
        for user in peers.into_iter().take(3){self.participants.insert(user.id.clone(),user);}
        self.status="Offline call preview · no devices or network active".into();self.chat=false;
    }
    pub fn show(&mut self, ctx: &egui::Context, _images: &mut crate::assets::Images) -> Vec<Value> {
        if self.share_picker {
            let mut open = true;
            egui::Window::new("Share a screen or window")
                .open(&mut open)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Choose exactly what to share.");
                    ui.checkbox(
                        &mut self.share_audio,
                        "Include system audio (other apps, excluding Eclipse)",
                    );
                    egui::ScrollArea::vertical()
                        .max_height(320.0)
                        .show(ui, |ui| {
                            for (index, source) in self.sources.iter().enumerate() {
                                ui.selectable_value(&mut self.source, index, &source.name);
                            }
                        });
                    if ui
                        .add_enabled(
                            !self.sources.is_empty(),
                            egui::Button::new(format!("Go Live · {}p {}fps",self.prefs.screen_height,self.prefs.screen_fps)),
                        )
                        .clicked()
                    {
                        match self.start_share() {
                            Ok(payload) => {
                                self.outbound.push(payload);
                                self.share_picker = false;
                            }
                            Err(error) => self.error = Some(error.into()),
                        }
                    }
                });
            if !open {
                self.share_picker = false;
            }
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
                audio.set_controls(muted||!hotkey_open(self.ptt.load(Ordering::Acquire),key_down), deafened);
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
    pub fn sync_roster(&mut self,users:Vec<User>){
        let Some(channel)=&self.channel else{return};
        let me=self.user.as_ref().map(|u|u.id.clone()).unwrap_or_default();
        if channel.guild_id.is_some()&&!users.is_empty(){self.participants.retain(|id,_|*id==me||users.iter().any(|u|&u.id==id));}
        for user in users.into_iter().filter(|u|!u.id.is_empty()).take(64){
            let entry=self.participants.entry(user.id.clone()).or_insert_with(||user.clone());
            if entry.username.is_empty(){*entry=user;}
        }
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
fn stream_key(channel: &Channel, user: &str) -> String {
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
        calls.sync_roster(vec![me.clone(),friend("2","husain"),friend("3","mahin")]);
        assert_eq!(calls.participants.len(),3);
        calls.sync_roster(vec![me.clone(),friend("2","husain")]);
        assert!(calls.participants.contains_key("1")&&calls.participants.contains_key("2")&&!calls.participants.contains_key("3"));
    }
}
