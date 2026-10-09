use crate::model::*;
use crossbeam_channel::{bounded, Receiver, Sender};
use eframe::egui;
use reqwest::blocking::{multipart, Client};
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    collections::{HashMap,VecDeque},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
use tungstenite::{connect, stream::MaybeTlsStream, Message as WsMessage};
use zeroize::Zeroizing;

const API: &str = "https://discord.com/api/v10";

pub fn network_check() -> Value {
    let result = (|| -> Result<Value, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| "HTTPS initialization failed")?;
        let response: Value = client
            .get(format!("{API}/gateway"))
            .send()
            .map_err(|_| "Discord HTTPS connection failed")?
            .error_for_status()
            .map_err(|_| "Discord returned an HTTP error")?
            .json()
            .map_err(|_| "Unexpected discovery response")?;
        let (mut socket, _) = connect("wss://gateway.discord.gg/?v=10&encoding=json")
            .map_err(|error| format!("Public WebSocket check failed: {error}"))?;
        match socket.get_mut() {
            MaybeTlsStream::Plain(stream) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
            }
            MaybeTlsStream::Rustls(stream) => {
                let _ = stream.sock.set_read_timeout(Some(Duration::from_secs(10)));
            }
            _ => {}
        }
        let frame = socket.read().map_err(|_| "Could not read Gateway Hello")?;
        let hello: Value =
            serde_json::from_str(frame.to_text().map_err(|_| "Unexpected Gateway frame")?)
                .map_err(|_| "Invalid Gateway JSON")?;
        let _ = socket.close(None);
        Ok(
            json!({"https":true,"gateway_hello":hello["op"]==10,"discovery_url":response["url"],"authenticated":false}),
        )
    })();
    match result {
        Ok(value) => value,
        Err(error) => json!({"error":error,"authenticated":false}),
    }
}

#[derive(Debug)]
pub enum Command {
    Request { key:String, method:Method, route:String, body:Option<Value> },
    Channels(String),
    History(String, Option<String>),
    /// Messages surrounding one message, used to jump to a pin outside the loaded history.
    Around(String, String),
    Send {
        channel: String,
        content: String,
        file: Option<PathBuf>,
        nonce: String,
        reference:Option<String>,
    },
    Edit {
        channel: String,
        id: String,
        content: String,
    },
    Delete {
        channel: String,
        id: String,
    },
    Reaction {
        channel: String,
        id: String,
        emoji: String,
        remove: bool,
    },
    Pins(String),
    OpenDm(String),
    Refresh,
    Emojis(String),
    Gifs(String),
    Ring(String, Arc<AtomicU64>, u64),
}

#[derive(Debug)]
pub enum Event {
    Data(String, Result<Value,String>),
    Account(String,Value),
    Connected(User, Vec<Guild>, Vec<Channel>),
    Channels(String, Vec<Channel>),
    History(String, Vec<Message>, bool),
    Around(String, Vec<Message>, String),
    Message(Message),
    Patch(Value),
    Deleted(String, String),
    Gateway(String),
    Error(String),
    Sent(Message),
    SendFailed(String),
    Pins(String, Vec<Message>),
    Dm(Channel),
    ReactionRefresh(String),
    Folders(crate::folders::Layout),
    FolderStatus(String),
    ProfilePatch(String, Value),
    Presence(Value),
    Signal(String, Value),
    Emojis(String, Vec<crate::media_picker::CustomEmoji>),
    Gifs(String, Vec<crate::media_picker::Gif>),
    Ringing,
}

pub struct Backend {
    pub tx: Sender<Command>,
    pub rx: Receiver<Event>,
    pub gateway: Sender<Value>,
    cancel: Arc<AtomicBool>,
}
impl Drop for Backend {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn emit(tx: &Sender<Event>, ctx: &egui::Context, event: Event) -> bool {
    let ok = tx.send(event).is_ok();
    ctx.request_repaint();
    ok
}

pub fn start(token: String, ctx: egui::Context) -> Backend {
    let token = Arc::new(Zeroizing::new(token));
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, commands) = bounded(32);
    let (events, rx) = bounded(256);
    let (gateway_tx, gateway_rx) = bounded(32);
    let cancel_worker = cancel.clone();
    thread::spawn(move || {
        let mut api = match Api::new(token.clone(), cancel_worker.clone()) {
            Ok(api) => api,
            Err(error) => {
                emit(&events, &ctx, Event::Error(error));
                return;
            }
        };
        match api.bootstrap() {
            Ok((user, guilds, dms)) => {
                if !emit(&events, &ctx, Event::Connected(user, guilds, dms)) {
                    return;
                }
                let (token, cancel, events, ctx) = (
                    token.clone(),
                    cancel_worker.clone(),
                    events.clone(),
                    ctx.clone(),
                );
                thread::spawn(move || gateway(token, cancel, events, ctx, gateway_rx));
            }
            Err(error) => {
                emit(&events, &ctx, Event::Error(error));
                return;
            }
        }
        // Optional account settings must never stall the text-chat command queue.
        let (folder_tx, folder_rx) = bounded::<()>(1);
        let _ = folder_tx.try_send(());
        let (folder_token, folder_cancel, folder_events, folder_ctx) = (
            token.clone(),
            cancel_worker.clone(),
            events.clone(),
            ctx.clone(),
        );
        thread::spawn(move || {
            let Ok(mut api) = Api::new(folder_token, folder_cancel.clone()) else {
                return;
            };
            while !folder_cancel.load(Ordering::Relaxed) {
                match folder_rx.recv_timeout(Duration::from_millis(400)) {
                    Ok(()) => {}
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                    Err(_) => break,
                }
                let event = match api.folders() {
                    Ok(layout) => Event::Folders(layout),
                    Err(_) => Event::FolderStatus(
                        "Folder import unavailable. Use Refresh servers, icons & folders to retry."
                            .into(),
                    ),
                };
                if !emit(&folder_events, &folder_ctx, event) {
                    break;
                }
            }
        });
        let mut queued=VecDeque::new();
        while !cancel_worker.load(Ordering::Relaxed) {
            if queued.is_empty(){match commands.recv_timeout(Duration::from_millis(400)){Ok(c)=>queued.push_back(c),Err(crossbeam_channel::RecvTimeoutError::Timeout)=>continue,Err(_)=>break}}
            queued.extend(commands.try_iter().take(64usize.saturating_sub(queued.len())));
            let now=Instant::now();
            let index=queued.iter().enumerate().filter(|(_,c)|command_ready(&api.limits,c).is_none_or(|ready|ready<=now)).min_by_key(|(i,c)|(command_priority(c),*i)).map(|(i,_)|i);
            let Some(index)=index else{thread::sleep(Duration::from_millis(100));continue;};
            let command=queued.remove(index).unwrap();
            if cancel_worker.load(Ordering::Relaxed) {
                break;
            }
            let is_send = matches!(&command, Command::Send { .. });
            let refresh_folders = matches!(&command, Command::Refresh);
            let event = api.command(command).unwrap_or_else(|error| {
                if is_send {
                    Event::SendFailed(error)
                } else {
                    Event::Error(error)
                }
            });
            if !emit(&events, &ctx, event) {
                break;
            }
            if refresh_folders {
                let _ = folder_tx.try_send(());
            }
        }
    });
    Backend {
        tx,
        rx,
        cancel,
        gateway: gateway_tx,
    }
}

// Discord buckets are shared by route and top-level resource, independently of global limits.
// https://docs.discord.com/developers/topics/rate-limits
#[derive(Default)]struct Limits{global:Option<Instant>,aliases:HashMap<String,String>,deadlines:HashMap<String,Instant>}
fn route_key(method:&Method,route:&str)->(String,String){
    let parts:Vec<_>=route.split('?').next().unwrap_or(route).trim_matches('/').split('/').collect();
    let major=if matches!(parts.first().copied(),Some("channels"|"guilds"|"webhooks")){parts.get(1).copied().unwrap_or("")}else{""};
    let path=parts.iter().enumerate().map(|(i,p)|if i!=1||major.is_empty(){if p.bytes().all(|b|b.is_ascii_digit())&&!p.is_empty(){"{id}"}else{p}}else{p}).collect::<Vec<_>>().join("/");
    (format!("{method} /{path}"),major.into())
}
impl Limits{
    fn key(&self,method:&Method,route:&str)->String{let(key,_)=route_key(method,route);self.aliases.get(&key).cloned().unwrap_or(key)}
    fn until(&self,method:&Method,route:&str)->Instant{self.global.into_iter().chain(self.deadlines.get(&self.key(method,route)).copied()).max().unwrap_or_else(||Instant::now()-Duration::from_secs(1))}
    fn observe(&mut self,method:&Method,route:&str,headers:&reqwest::header::HeaderMap){
        let now=Instant::now();self.deadlines.retain(|_,v|*v>now);
        let(key,major)=route_key(method,route);
        if let Some(bucket)=headers.get("x-ratelimit-bucket").and_then(|v|v.to_str().ok()).filter(|v|v.len()<200){if self.aliases.len()<2048||self.aliases.contains_key(&key){self.aliases.insert(key,format!("{bucket}:{major}"));}}
        if headers.get("x-ratelimit-remaining").and_then(|v|v.to_str().ok())==Some("0"){if let Some(delay)=headers.get("x-ratelimit-reset-after").and_then(|v|v.to_str().ok()).and_then(|v|v.parse::<f64>().ok()).filter(|v|v.is_finite()&&*v>=0.){self.delay(method,route,delay,false);}}
    }
    fn delay(&mut self,method:&Method,route:&str,seconds:f64,global:bool){let until=Instant::now()+Duration::from_secs_f64(seconds.clamp(0.,3600.)+0.1);if global{self.global=Some(self.global.unwrap_or(until).max(until));}else{let key=self.key(method,route);self.deadlines.entry(key).and_modify(|old|*old=(*old).max(until)).or_insert(until);}}
}
fn command_priority(command:&Command)->u8{match command{Command::History(..)|Command::Around(..)|Command::Channels(..)=>1,Command::Request{method,..}if *method==Method::GET=>2,_=>0}}
fn command_ready(limits:&Limits,command:&Command)->Option<Instant>{match command{Command::History(id,_)|Command::Around(id,_)=>Some(limits.until(&Method::GET,&format!("/channels/{id}/messages"))),Command::Channels(id)=>Some(limits.until(&Method::GET,&format!("/guilds/{id}/channels"))),Command::Request{method,route,..}=>Some(limits.until(method,route)),_=>limits.global}}
struct Api {
    client: Client,
    token: Arc<Zeroizing<String>>,
    cancel: Arc<AtomicBool>,
    limits: Limits,
    base: String,
}
impl Api {
    fn new(token: Arc<Zeroizing<String>>, cancel: Arc<AtomicBool>) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .user_agent("EclipseNative/0.8 (Windows; independent native client)")
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "Could not initialize HTTPS.".to_owned())?;
        Ok(Self {
            client,
            token,
            cancel,
            limits: Limits::default(),
            base: API.into(),
        })
    }
    /// Discord's own upload flow: ask for an upload slot (Discord checks the account's and the
    /// server's size limit here, before any data moves), stream the file to the returned
    /// storage URL without a request timeout, and return the name the message refers to.
    fn upload(&mut self, channel: &str, path: &PathBuf, filename: &str) -> Result<String, String> {
        let size = std::fs::metadata(path).map_err(|_| "The selected file is no longer available.".to_owned())?.len();
        if size > MAX_UPLOAD {
            return Err("Choose a file smaller than 1 GB.".into());
        }
        let slot: Value = self.request(
            Method::POST,
            &format!("/channels/{channel}/attachments"),
            Some(json!({"files":[{"id":"0","filename":filename,"file_size":size,"is_clip":false}]})),
            None,
        )?;
        let target = &slot["attachments"][0];
        let (Some(url), Some(uploaded)) = (target["upload_url"].as_str(), target["upload_filename"].as_str()) else {
            return Err("Discord did not provide an upload slot for this file.".into());
        };
        if !upload_url(url) {
            return Err("Discord returned an unexpected upload address.".into());
        }
        let file = std::fs::File::open(path).map_err(|_| "Could not open the selected file.".to_owned())?;
        // Large files can take minutes: only connecting is time-limited.
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "Could not initialize HTTPS.".to_owned())?;
        let response = client
            .put(url)
            .header("Content-Type", "application/octet-stream")
            .header("Content-Length", size)
            .body(reqwest::blocking::Body::sized(file, size))
            .send()
            .map_err(|_| "The upload was interrupted. Check your connection and try again.".to_owned())?;
        if !response.status().is_success() {
            return Err(format!("The upload failed ({}).", response.status().as_u16()));
        }
        Ok(uploaded.to_owned())
    }
    fn wait(&self, until: Instant) -> Result<(), String> {
        while Instant::now() < until {
            if self.cancel.load(Ordering::Relaxed) {
                return Err("Disconnected.".into());
            }
            thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }
    fn request<T: DeserializeOwned>(
        &mut self,
        method: Method,
        route: &str,
        body: Option<Value>,
        file: Option<&PathBuf>,
    ) -> Result<T, String> {
        for _ in 0..3 {
            self.wait(self.limits.until(&method,route))?;
            if self.cancel.load(Ordering::Relaxed) {
                return Err("Disconnected.".into());
            }
            let mut request = self
                .client
                .request(method.clone(), format!("{}{route}", self.base))
                .header("Authorization", self.token.as_str());
            if let Some(path) = file {
                let metadata = std::fs::metadata(path)
                    .map_err(|_| "The selected file is no longer available.".to_owned())?;
                if metadata.len() > 512 * 1024 * 1024 {
                    return Err("Choose a file smaller than 512 MB. Discord may apply a lower account limit.".into());
                }
                let form = multipart::Form::new()
                    .text(
                        "payload_json",
                        body.as_ref().unwrap_or(&json!({})).to_string(),
                    )
                    .file("files[0]", path)
                    .map_err(|_| "Could not open the selected file.".to_owned())?;
                request = request.multipart(form);
            } else if let Some(body) = &body {
                request = request.json(body);
            }
            let response = request.send().map_err(|error| {
                if error.is_timeout() {
                    "The request timed out. Check the channel before retrying a message.".to_owned()
                } else {
                    "Could not reach Discord over HTTPS. Check your connection.".to_owned()
                }
            })?;
            let status = response.status();
            self.limits.observe(&method,route,response.headers());
            let global=response.headers().get("x-ratelimit-global").and_then(|v|v.to_str().ok())==Some("true");
            if status.as_u16() == 204 {
                return serde_json::from_value(Value::Null)
                    .map_err(|_| "Unexpected empty response.".into());
            }
            // Limit response size: history requests are capped and the UI never downloads media automatically.
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(
                &mut std::io::Read::take(response, 8 * 1024 * 1024),
                &mut bytes,
            )
            .map_err(|_| "Could not read Discord's response.".to_owned())?;
            let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
                format!(
                    "Discord returned an unexpected response (HTTP {}).",
                    status.as_u16()
                )
            })?;
            if status.as_u16() == 429 {
                let retry = value["retry_after"].as_f64().unwrap_or(5.0);
                let retry = if retry.is_finite() {
                    retry.clamp(0.5, 3600.0)
                } else {
                    5.0
                };
                self.limits.delay(&method,route,retry,global||value["global"].as_bool()==Some(true));
                continue;
            }
            if !status.is_success() {
                return Err(match status.as_u16() {
                    401 => "Discord rejected the token. Disconnect and check your account token.".into(),
                    403 => "Discord denied access. You may lack channel permissions, or this API may be restricted.".into(),
                    404 => "That channel or message is no longer available.".into(),
                    413 => "The attachment exceeds Discord's upload limit for this account.".into(),
                    n => format!("Discord returned HTTP {}: {}", n, value["message"].as_str().unwrap_or("Request failed.").chars().take(180).collect::<String>()),
                });
            }
            return serde_json::from_value(value)
                .map_err(|_| "Discord's response format is unsupported by this build.".into());
        }
        Err("Discord is rate limiting requests. Wait before trying again.".into())
    }
    fn folders(&mut self) -> Result<crate::folders::Layout, String> {
        if let Ok(value) =
            self.request::<Value>(Method::GET, "/users/@me/settings-proto/1", None, None)
        {
            if let Some(encoded) = value["settings"].as_str() {
                if let Some(layout) = crate::folders::decode(encoded)? {
                    return Ok(layout);
                }
                // A valid full settings object with no guild_folders means no saved folders.
                return Ok(crate::folders::Layout::default());
            }
        }
        let value: Value = self.request(Method::GET, "/users/@me/settings", None, None)?;
        crate::folders::legacy(&value).ok_or_else(|| "No server folder settings returned.".into())
    }
    fn bootstrap(&mut self) -> Result<(User, Vec<Guild>, Vec<Channel>), String> {
        let user = self.request(Method::GET, "/users/@me", None, None)?;
        let mut guilds = Vec::new();
        let mut after = String::new();
        loop {
            let route = format!(
                "/users/@me/guilds?limit=200{}",
                if after.is_empty() {
                    String::new()
                } else {
                    format!("&after={after}")
                }
            );
            let page: Vec<Guild> = self.request(Method::GET, &route, None, None)?;
            let len = page.len();
            after = page.last().map(|g| g.id.clone()).unwrap_or_default();
            guilds.extend(page);
            if len < 200 || guilds.len() >= 1000 {
                break;
            }
        }
        let dms = self.request(Method::GET, "/users/@me/channels", None, None)?;
        Ok((user, guilds, dms))
    }
    fn command(&mut self, command: Command) -> Result<Event, String> {
        Ok(match command {
            Command::Request{key,method,route,body}=>{
                if !route.starts_with('/')||route.starts_with("//")||route.contains("..")||route.contains(['\r','\n','#'])||key.len()>160 {return Ok(Event::Data(key,Err("Invalid Discord API route.".into())));}
                let result=self.request::<Value>(method,&route,body,None).map(|mut data|{crate::community::scrub(&mut data);data});
                Event::Data(key,result)
            }
            Command::Emojis(guild) => Event::Emojis(
                guild.clone(),
                self.request(Method::GET, &format!("/guilds/{guild}/emojis"), None, None)?,
            ),
            Command::Gifs(query) => {
                let mut url = reqwest::Url::parse("https://discord.com/gifs/search").unwrap();
                url.query_pairs_mut()
                    .append_pair("q", &query)
                    .append_pair("media_format", "tinygif")
                    .append_pair("provider", "klipy")
                    .append_pair("locale", "en-US")
                    .append_pair("limit", "20");
                Event::Gifs(
                    query,
                    self.request(
                        Method::GET,
                        &format!("/gifs/search?{}", url.query().unwrap_or_default()),
                        None,
                        None,
                    )?,
                )
            }
            Command::Ring(channel, intent, epoch) => {
                if intent.load(Ordering::Acquire) != epoch {
                    return Ok(Event::Ringing);
                }
                let _: Value = self.request(
                    Method::POST,
                    &format!("/channels/{channel}/call/ring"),
                    Some(json!({"recipients":null})),
                    None,
                )?;
                Event::Ringing
            }
            Command::Channels(id) => {
                let channels =
                    self.request(Method::GET, &format!("/guilds/{id}/channels"), None, None)?;
                Event::Channels(id, channels)
            }
            Command::History(id, before) => {
                let older = before.is_some();
                let route = format!(
                    "/channels/{id}/messages?limit=50{}",
                    before.map(|s| format!("&before={s}")).unwrap_or_default()
                );
                let messages = self.request(Method::GET, &route, None, None)?;
                Event::History(id, messages, older)
            }
            Command::Around(id, message) => {
                let messages = self.request(Method::GET, &format!("/channels/{id}/messages?limit=50&around={message}"), None, None)?;
                Event::Around(id, messages, message)
            }
            Command::Send {
                channel,
                content,
                file,
                nonce,
                reference,
            } => {
                let mut payload = json!({"content":content, "nonce":nonce, "enforce_nonce":true});
                if let Some(id)=reference{payload["message_reference"]=json!({"message_id":id,"channel_id":channel,"fail_if_not_exists":false});payload["allowed_mentions"]=json!({"parse":["users","roles","everyone"],"replied_user":false});}
                if let Some(file) = &file {
                    let filename = file
                        .file_name()
                        .and_then(|s| s.to_str())
                        .ok_or("Invalid filename.")?;
                    let uploaded = self.upload(&channel, file, filename)?;
                    payload["attachments"] = json!([{"id":"0","filename":filename,"uploaded_filename":uploaded}]);
                }
                let message = self.request(
                    Method::POST,
                    &format!("/channels/{channel}/messages"),
                    Some(payload),
                    None,
                )?;
                Event::Sent(message)
            }
            Command::Edit {
                channel,
                id,
                content,
            } => Event::Message(self.request(
                Method::PATCH,
                &format!("/channels/{channel}/messages/{id}"),
                Some(json!({"content":content})),
                None,
            )?),
            Command::Delete { channel, id } => {
                let _: Value = self.request(
                    Method::DELETE,
                    &format!("/channels/{channel}/messages/{id}"),
                    None,
                    None,
                )?;
                Event::Deleted(channel, id)
            }
            Command::Reaction {
                channel,
                id,
                emoji,
                remove,
            } => {
                let encoded: String =
                    reqwest::Url::parse_with_params("https://discord.com", &[("e", &emoji)])
                        .unwrap()
                        .query()
                        .unwrap()[2..]
                        .to_owned();
                let _: Value = self.request(
                    if remove { Method::DELETE } else { Method::PUT },
                    &format!("/channels/{channel}/messages/{id}/reactions/{encoded}/@me"),
                    None,
                    None,
                )?;
                Event::ReactionRefresh(channel)
            }
            Command::Pins(channel) => {
                let response: Value = self.request(
                    Method::GET,
                    &format!("/channels/{channel}/messages/pins?limit=50"),
                    None,
                    None,
                )?;
                let messages = response["items"]
                    .as_array()
                    .ok_or("Discord's pin response format is unsupported.")?
                    .iter()
                    .map(|item| serde_json::from_value(item["message"].clone()))
                    .collect::<Result<Vec<Message>, _>>()
                    .map_err(|_| "Could not read pinned messages.".to_owned())?;
                Event::Pins(channel, messages)
            }
            Command::OpenDm(id) => Event::Dm(self.request(
                Method::POST,
                "/users/@me/channels",
                Some(json!({"recipient_id":id})),
                None,
            )?),
            Command::Refresh => {
                let (user, guilds, dms) = self.bootstrap()?;
                Event::Connected(user, guilds, dms)
            }
        })
    }
}

fn settings_folders(data: &Value) -> Option<crate::folders::Layout> {
    let settings = &data["settings"];
    if settings["type"].as_u64() != Some(1) {
        return None;
    }
    crate::folders::decode(settings["proto"].as_str()?)
        .ok()
        .flatten()
}
fn ready_folders(data: &Value) -> Option<crate::folders::Layout> {
    data["user_settings_proto"]
        .as_str()
        .and_then(|s| crate::folders::decode(s).ok().flatten())
        .or_else(|| crate::folders::legacy(&data["user_settings"]))
}

fn gateway(
    token: Arc<Zeroizing<String>>,
    cancel: Arc<AtomicBool>,
    tx: Sender<Event>,
    ctx: egui::Context,
    commands: Receiver<Value>,
) {
    let mut session: Option<String> = None;
    let mut resume_url: Option<String> = None;
    let mut seq: Option<i64> = None;
    let mut failures = 0u64;
    while !cancel.load(Ordering::Relaxed) {
        emit(
            &tx,
            &ctx,
            Event::Gateway(if failures == 0 {
                "Connecting live updates…".into()
            } else {
                "Reconnecting live updates…".into()
            }),
        );
        let result = (|| -> Result<(), String> {
            let mut url =
                reqwest::Url::parse(resume_url.as_deref().unwrap_or("wss://gateway.discord.gg/"))
                    .map_err(|_| "Invalid Gateway address")?;
            url.query_pairs_mut()
                .append_pair("v", "10")
                .append_pair("encoding", "json");
            let (mut socket, _) =
                connect(url.as_str()).map_err(|_| "Live connection unavailable".to_owned())?;
            match socket.get_mut() {
                MaybeTlsStream::Plain(stream) => {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
                }
                MaybeTlsStream::Rustls(stream) => {
                    let _ = stream.sock.set_read_timeout(Some(Duration::from_secs(1)));
                    let _ = stream.sock.set_write_timeout(Some(Duration::from_secs(10)));
                }
                _ => {}
            }
            let mut interval = Duration::from_secs(40);
            let mut next_heartbeat = Instant::now() + interval;
            let mut acknowledged = true;
            let mut live = false;
            let mut sent_commands = std::collections::VecDeque::<Instant>::new();
            while !cancel.load(Ordering::Relaxed) {
                while sent_commands
                    .front()
                    .is_some_and(|at| at.elapsed() >= Duration::from_secs(60))
                {
                    sent_commands.pop_front();
                }
                if live {
                    for _ in 0..8 {
                        if sent_commands.len() >= 100 {
                            break;
                        }
                        let Ok(command) = commands.try_recv() else {
                            break;
                        };
                        socket
                            .send(WsMessage::Text(command.to_string().into()))
                            .map_err(|_| "Gateway command failed".to_owned())?;
                        sent_commands.push_back(Instant::now());
                    }
                }
                if Instant::now() >= next_heartbeat {
                    if !acknowledged {
                        return Err("Heartbeat timed out".into());
                    }
                    socket
                        .send(WsMessage::Text(json!({"op":1,"d":seq}).to_string().into()))
                        .map_err(|_| "Heartbeat failed".to_owned())?;
                    acknowledged = false;
                    next_heartbeat = Instant::now() + interval;
                }
                let frame = match socket.read() {
                    Ok(frame) => frame,
                    Err(tungstenite::Error::Io(error))
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) =>
                    {
                        continue
                    }
                    Err(_) => return Err("Connection ended".into()),
                };
                if let WsMessage::Close(close) = &frame {
                    if close
                        .as_ref()
                        .is_some_and(|f| matches!(u16::from(f.code), 4004 | 4013 | 4014))
                    {
                        emit(&tx, &ctx, Event::Gateway("Discord rejected the live connection. Manual refresh is still available.".into()));
                        return Ok(());
                    }
                    return Err("Server closed connection".into());
                }
                let WsMessage::Text(text) = frame else {
                    continue;
                };
                let Ok(value) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                if let Some(s) = value["s"].as_i64() {
                    seq = Some(s);
                }
                match value["op"].as_u64() {
                    Some(10) => {
                        interval = Duration::from_millis(
                            value["d"]["heartbeat_interval"]
                                .as_u64()
                                .unwrap_or(40000)
                                .clamp(1000, 120000),
                        );
                        next_heartbeat = Instant::now() + interval / 2;
                        let payload = if let Some(session_id) = &session {
                            json!({"op":6,"d":{"token":token.as_str(),"session_id":session_id,"seq":seq}})
                        } else {
                            json!({"op":2,"d":{"token":token.as_str(),"properties":{"os":"Windows","browser":"Eclipse Native","device":"Eclipse Native"},"compress":false,"large_threshold":50}})
                        };
                        socket
                            .send(WsMessage::Text(payload.to_string().into()))
                            .map_err(|_| "Handshake failed".to_owned())?;
                    }
                    Some(11) => acknowledged = true,
                    Some(1) => {
                        socket
                            .send(WsMessage::Text(json!({"op":1,"d":seq}).to_string().into()))
                            .map_err(|_| "Heartbeat failed".to_owned())?;
                        acknowledged = false;
                        next_heartbeat = Instant::now() + interval;
                    }
                    Some(7) => return Err("Discord requested reconnect".into()),
                    Some(9) => {
                        if value["d"].as_bool() != Some(true) {
                            session = None;
                            seq = None;
                            resume_url = None;
                        }
                        return Err("Session expired".into());
                    }
                    Some(0) => {
                        let data = &value["d"];
                        let kind=value["t"].as_str().unwrap_or_default();
                        if matches!(kind,"READY"|"READY_SUPPLEMENTAL"|"GUILD_CREATE"|"GUILD_UPDATE"|"GUILD_MEMBER_ADD"|"GUILD_MEMBER_UPDATE"|"GUILD_MEMBER_REMOVE"|"GUILD_MEMBERS_CHUNK"|"GUILD_MEMBER_LIST_UPDATE"|"GUILD_ROLE_CREATE"|"GUILD_ROLE_UPDATE"|"GUILD_ROLE_DELETE"|"RELATIONSHIP_ADD"|"RELATIONSHIP_UPDATE"|"RELATIONSHIP_REMOVE"|"USER_SETTINGS_UPDATE"|"MESSAGE_ACK"|"QUESTS_USER_STATUS_UPDATE"|"GUILD_EMOJIS_UPDATE"|"CHANNEL_CREATE"|"CHANNEL_UPDATE"|"CHANNEL_DELETE"|"GUILD_DELETE") {
                            if !emit(&tx,&ctx,Event::Account(kind.into(),data.clone())){return Ok(());}
                        }
                        let event = match value["t"].as_str().unwrap_or("") {
                            "READY" => {
                                live = true;
                                emit(&tx, &ctx, Event::Presence(data.clone()));
                                if let Some(layout) = ready_folders(data) {
                                    if !emit(&tx, &ctx, Event::Folders(layout)) {
                                        return Ok(());
                                    }
                                }
                                session = data["session_id"].as_str().map(str::to_owned);
                                resume_url = data["resume_gateway_url"]
                                    .as_str()
                                    .filter(|address| {
                                        reqwest::Url::parse(address).is_ok_and(|u| {
                                            u.scheme() == "wss"
                                                && u.host_str()
                                                    .is_some_and(|h| h.ends_with(".discord.gg"))
                                        })
                                    })
                                    .map(str::to_owned);
                                failures = 0;
                                Some(Event::Gateway("Live".into()))
                            }
                            "RESUMED" => {
                                live = true;
                                failures = 0;
                                Some(Event::Gateway("Live".into()))
                            }
                            "PRESENCE_UPDATE"
                            | "READY_SUPPLEMENTAL"
                            | "GUILD_CREATE"
                            | "GUILD_MEMBERS_CHUNK"
                            | "GUILD_MEMBER_LIST_UPDATE" => Some(Event::Presence(data.clone())),
                            "VOICE_STATE_UPDATE"
                            | "VOICE_SERVER_UPDATE"
                            | "STREAM_CREATE"
                            | "STREAM_SERVER_UPDATE"
                            | "STREAM_DELETE"
                            | "CALL_CREATE"
                            | "CALL_UPDATE"
                            | "CALL_DELETE" => Some(Event::Signal(
                                value["t"].as_str().unwrap_or_default().into(),
                                data.clone(),
                            )),
                            "MESSAGE_CREATE" => serde_json::from_value(data.clone())
                                .ok()
                                .map(Event::Message),
                            "MESSAGE_UPDATE"
                            | "MESSAGE_DELETE_BULK"
                            | "MESSAGE_REACTION_ADD"
                            | "MESSAGE_REACTION_REMOVE"
                            | "MESSAGE_REACTION_REMOVE_ALL" => {
                                Some(Event::Patch(json!({"type":value["t"], "data":data})))
                            }
                            "MESSAGE_DELETE" => Some(Event::Deleted(
                                data["channel_id"].as_str().unwrap_or("").into(),
                                data["id"].as_str().unwrap_or("").into(),
                            )),
                            "USER_SETTINGS_PROTO_UPDATE" => {
                                settings_folders(data).map(Event::Folders)
                            }
                            "USER_SETTINGS_UPDATE" => {
                                crate::folders::legacy(data).map(Event::Folders)
                            }
                            "USER_UPDATE" | "GUILD_UPDATE" => Some(Event::ProfilePatch(
                                value["t"].as_str().unwrap_or("").into(),
                                data.clone(),
                            )),
                            _ => None,
                        };
                        if let Some(event) = event {
                            if !emit(&tx, &ctx, event) {
                                return Ok(());
                            }
                        }
                    }
                    _ => {}
                }
            }
            let _ = socket.close(None);
            Ok(())
        })();
        if result.is_ok() {
            break;
        }
        failures += 1;
        let until = Instant::now() + Duration::from_secs((failures * 3).min(30));
        while Instant::now() < until && !cancel.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(200));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, Read, Write};
    use std::net::TcpListener;

    fn mock(responses: Vec<&'static str>) -> (Api, Receiver<String>, thread::JoinHandle<()>) {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", server.local_addr().unwrap());
        let (tx, rx) = bounded(8);
        let handle = thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = server.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut reader = std::io::BufReader::new(&mut stream);
                let mut request = String::new();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some(n) = line.to_lowercase().strip_prefix("content-length: ") {
                        length = n.trim().parse().unwrap();
                    }
                    request.push_str(&line);
                }
                let mut body = vec![0u8; length];
                reader.read_exact(&mut body).unwrap();
                request.push_str(&String::from_utf8_lossy(&body));
                let _ = tx.send(request);
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        let mut api = Api::new(
            Arc::new(Zeroizing::new("not-a-real-account-token".into())),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        api.base = base;
        (api, rx, handle)
    }
    #[test]fn route_buckets_do_not_stall_other_channels_but_global_limits_do(){
        let mut limits=Limits::default();let mut headers=reqwest::header::HeaderMap::new();headers.insert("x-ratelimit-bucket","history".parse().unwrap());headers.insert("x-ratelimit-remaining","1".parse().unwrap());
        limits.observe(&Method::GET,"/channels/123/messages?before=9",&headers);limits.observe(&Method::GET,"/channels/456/messages",&headers);
        headers.insert("x-ratelimit-remaining","0".parse().unwrap());headers.insert("x-ratelimit-reset-after","10".parse().unwrap());limits.observe(&Method::GET,"/channels/123/messages?limit=50",&headers);
        assert!(limits.until(&Method::GET,"/channels/123/messages?before=8")>Instant::now());assert!(limits.until(&Method::GET,"/channels/456/messages")<=Instant::now());assert!(limits.until(&Method::GET,"/guilds/123/channels")<=Instant::now());
        limits.delay(&Method::GET,"/channels/123/messages",2.,true);assert!(limits.until(&Method::GET,"/channels/456/messages")>Instant::now());
        assert!(command_priority(&Command::History("123".into(),None))<command_priority(&Command::Request{key:"roles".into(),method:Method::GET,route:"/guilds/123/roles".into(),body:None}));
    }
    #[test]
    fn sends_message_with_nonce_and_auth_to_expected_route() {
        let body = r#"{"id":"123","channel_id":"456","content":"hello","author":{"id":"me","username":"me"}}"#;
        let response=Box::leak(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).into_boxed_str());
        let (mut api, rx, handle) = mock(vec![response]);
        let event = api
            .command(Command::Send {
                channel: "456".into(),
                content: "hello".into(),
                file: None,
                nonce: "999".into(),
                reference:Some("321".into()),
            })
            .unwrap();
        assert!(matches!(event,Event::Sent(Message{id,..}) if id=="123"));
        let request = rx.recv().unwrap();
        assert!(request.starts_with("POST /channels/456/messages HTTP/1.1"));
        assert!(request
            .to_lowercase()
            .contains("authorization: not-a-real-account-token"));
        assert!(request.contains("\"enforce_nonce\":true"));
        assert!(request.contains("\"message_id\":\"321\""));
        assert!(request.contains("\"replied_user\":false"));
        assert!(request.contains("\"nonce\":\"999\""));
        handle.join().unwrap();
    }
    #[test]
    fn gif_search_uses_current_provider_and_preserves_preview() {
        let body = r#"[{"url":"https://klipy.com/gifs/cat","preview":"https://static.klipy.com/cat.gif","gif_src":null,"title":"Cat"}]"#;
        let response=Box::leak(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).into_boxed_str());
        let (mut api, rx, handle) = mock(vec![response]);
        let event = api.command(Command::Gifs("cat & dog".into())).unwrap();
        assert!(
            matches!(event,Event::Gifs(_,gifs)if gifs.len()==1&&gifs[0].image()=="https://static.klipy.com/cat.gif")
        );
        let request = rx.recv().unwrap();
        assert!(request.starts_with("GET /gifs/search?"));
        assert!(request.contains("q=cat+%26+dog"));
        assert!(request.contains("provider=klipy"));
        assert!(request.contains("media_format=tinygif"));
        handle.join().unwrap();
    }
    #[test]
    fn honors_429_then_accepts_empty_delete_response() {
        let(mut api,rx,handle)=mock(vec![
            "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 21\r\nConnection: close\r\n\r\n{\"retry_after\":0.001}",
            "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n",
        ]);
        let start = Instant::now();
        assert!(matches!(
            api.command(Command::Delete {
                channel: "456".into(),
                id: "123".into()
            })
            .unwrap(),
            Event::Deleted(_, _)
        ));
        assert!(start.elapsed() >= Duration::from_millis(500));
        assert_eq!(rx.len(), 2);
        handle.join().unwrap();
    }
    #[test]
    fn auth_errors_do_not_echo_server_body_or_token() {
        let body = r#"{"message":"not-a-real-account-token"}"#;
        let response = Box::leak(
            format!(
                "HTTP/1.1 401 Unauthorized\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .into_boxed_str(),
        );
        let (mut api, _, handle) = mock(vec![response]);
        let error = api
            .request::<Value>(Method::GET, "/users/@me", None, None)
            .unwrap_err();
        assert!(error.contains("rejected"));
        assert!(!error.contains("not-a-real-account-token"));
        handle.join().unwrap();
    }
    #[test]
    fn cancellation_prevents_network_requests() {
        let mut api = Api::new(
            Arc::new(Zeroizing::new("not-a-real-account-token".into())),
            Arc::new(AtomicBool::new(true)),
        )
        .unwrap();
        assert_eq!(
            api.request::<Value>(Method::GET, "/users/@me", None, None)
                .unwrap_err(),
            "Disconnected."
        );
    }
    #[test]
    fn imports_account_folders_and_accepts_only_relevant_gateway_updates() {
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode([
            0x72, 0x10, 0x0A, 0x0E, 0x0A, 0x08, 0x01, 0, 0, 0, 0, 0, 0, 0, 0x12, 0x02, 0x08, 0x07,
        ]);
        let body = json!({"settings":encoded}).to_string();
        let response=Box::leak(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).into_boxed_str());
        let (mut api, rx, handle) = mock(vec![response]);
        let layout = api.folders().unwrap();
        assert_eq!(layout.folders[0].guild_ids, vec!["1"]);
        assert_eq!(layout.folders[0].id.as_deref(), Some("7"));
        assert!(rx
            .recv()
            .unwrap()
            .starts_with("GET /users/@me/settings-proto/1 HTTP/1.1"));
        handle.join().unwrap();
        assert!(
            settings_folders(&json!({"settings":{"type":1,"proto":encoded},"partial":true}))
                .is_some()
        );
        assert!(
            settings_folders(&json!({"settings":{"type":2,"proto":encoded},"partial":true}))
                .is_none()
        );
        assert!(settings_folders(
            &json!({"settings":{"type":1,"proto":"CgIIAQ=="},"partial":true})
        )
        .is_none());
        assert!(ready_folders(&json!({"user_settings_proto":encoded})).is_some());
    }
    #[test]
    fn falls_back_to_legacy_folders_without_writing_settings() {
        let body = r#"{"guild_folders":[{"id":7,"name":"Art","color":42,"guild_ids":["1"]}],"guild_positions":["1"]}"#;
        let response=Box::leak(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).into_boxed_str());
        let (mut api, rx, handle) = mock(vec![
            "HTTP/1.1 404 Not Found\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
            response,
        ]);
        let layout = api.folders().unwrap();
        assert_eq!(layout.folders[0].name.as_deref(), Some("Art"));
        assert!(rx
            .recv()
            .unwrap()
            .starts_with("GET /users/@me/settings-proto/1 "));
        assert!(rx.recv().unwrap().starts_with("GET /users/@me/settings "));
        handle.join().unwrap();
    }
}

/// Eclipse's own ceiling; Discord applies the account's and the server's real limit when the
/// upload slot is requested (Nitro allows the most).
const MAX_UPLOAD: u64 = 1024 * 1024 * 1024;
/// Upload slots point at Discord's Google Cloud Storage bucket over HTTPS.
fn upload_url(url: &str) -> bool {
    reqwest::Url::parse(url).is_ok_and(|u| {
        u.scheme() == "https"
            && u.host_str().is_some_and(|host| host == "storage.googleapis.com" || host.ends_with(".storage.googleapis.com") || host == "discord-attachments-uploads-prd.storage.googleapis.com")
            && u.username().is_empty()
            && u.password().is_none()
    })
}
#[cfg(test)]
mod upload_tests {
    #[test]
    fn upload_slots_must_be_discords_storage_over_https() {
        assert!(super::upload_url("https://discord-attachments-uploads-prd.storage.googleapis.com/abc/file.png?upload_id=1"));
        assert!(!super::upload_url("http://discord-attachments-uploads-prd.storage.googleapis.com/abc"));
        assert!(!super::upload_url("https://storage.googleapis.com.evil.com/abc"));
        assert!(!super::upload_url("https://evil.com/abc"));
    }
}
