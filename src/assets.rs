//! Small, memory-only public CDN cache. It never receives an account token.
use crate::model::{Channel, Guild, User};
use crossbeam_channel::{bounded, Receiver, Sender};
use eframe::egui::{self, ColorImage, TextureHandle, TextureId};
use image::{AnimationDecoder, ImageDecoder};
use std::{
    collections::{HashMap, HashSet},
    io::{Cursor, Read},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

pub const TEXTURE_LIMIT: usize = 128;
pub const PIXEL_BUDGET: usize = 48 * 1024 * 1024;
const ASSET_BUDGET: usize = 12 * 1024 * 1024;
const FRAME_LIMIT: usize = 360;
const FRAME_INTERVAL: Duration = Duration::from_micros(16_667);
const GIF_LIMIT: u64 = 16 * 1024 * 1024;
const IMAGE_SIZE: u32 = 1024;
// Small icons and large artwork receive separate display-sized cache entries.
fn resolution(size: egui::Vec2, scale: f32) -> u32 {
    let pixels = (size.max_elem() * scale).ceil().clamp(1., 1024.) as u32;
    [64, 128, 256, 512, 1024].into_iter().find(|n| *n >= pixels).unwrap_or(1024)
}
fn cache_key(url: &str, size: u32) -> String { format!("{url}#eclipse-resolution={size}") }
fn source_key(key: &str) -> (&str, u32) {
    key.rsplit_once("#eclipse-resolution=").and_then(|(url,n)|n.parse::<u32>().ok().map(|n|(url,n.clamp(16,1024)))).unwrap_or((key,128))
}
fn snowflake(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 20
        && s.bytes().all(|c| c.is_ascii_digit())
        && s.parse::<u64>().is_ok()
}
fn hash(s: &str) -> bool {
    let s = s.strip_prefix("a_").unwrap_or(s);
    s.len() == 32 && s.bytes().all(|c| c.is_ascii_hexdigit())
}
fn suffix(hash: &str) -> &'static str {
    if hash.starts_with("a_") {
        "gif?size=256"
    } else {
        "png?size=256"
    }
}
pub fn avatar_url(user: &User, guild: Option<&str>, member_avatar: Option<&str>) -> Option<String> {
    if !snowflake(&user.id) {
        return None;
    }
    if let (Some(guild), Some(avatar)) = (
        guild.filter(|g| snowflake(g)),
        member_avatar.filter(|h| hash(h)),
    ) {
        return Some(format!(
            "https://cdn.discordapp.com/guilds/{guild}/users/{}/avatars/{avatar}.{}",
            user.id,
            suffix(avatar)
        ));
    }
    if let Some(avatar) = user.avatar.as_deref().filter(|h| hash(h)) {
        return Some(format!(
            "https://cdn.discordapp.com/avatars/{}/{avatar}.{}",
            user.id,
            suffix(avatar)
        ));
    }
    let index = match user.discriminator.parse::<u32>() {
        Ok(n) if n != 0 => n % 5,
        _ => ((user.id.parse::<u64>().ok()? >> 22) % 6) as u32,
    };
    Some(format!(
        "https://cdn.discordapp.com/embed/avatars/{index}.png?size=128"
    ))
}
pub fn guild_url(guild: &Guild) -> Option<String> {
    let icon = guild.icon.as_deref().filter(|h| hash(h))?;
    snowflake(&guild.id).then(|| {
        format!(
            "https://cdn.discordapp.com/icons/{}/{icon}.{}",
            guild.id,
            suffix(icon)
        )
    })
}
pub fn channel_url(channel: &Channel) -> Option<String> {
    let icon = channel.icon.as_deref().filter(|h| hash(h))?;
    snowflake(&channel.id).then(|| {
        format!(
            "https://cdn.discordapp.com/channel-icons/{}/{icon}.{}",
            channel.id,
            suffix(icon)
        )
    })
}
pub fn public_url(url: &str) -> bool {
    if url.len() > 2048 {
        return false;
    }
    reqwest::Url::parse(url).is_ok_and(|u| {
        u.scheme() == "https"
            && u.host_str().is_some_and(|host| match host {
                "cdn.discordapp.com"
                | "media.discordapp.net"
                | "images-ext-1.discordapp.net"
                | "images-ext-2.discordapp.net" => true,
                "media.tenor.com" | "c.tenor.com" | "media.klipy.com" | "static.klipy.com"
                | "static1.klipy.com" | "static2.klipy.com" => true,
                "cdnjs.cloudflare.com" => u.path().starts_with("/ajax/libs/twemoji/14.0.2/72x72/"),
                _ => false,
            })
            && u.username().is_empty()
            && u.password().is_none()
            && u.port().is_none()
            && !u.path().contains("..")
            && [".gif", ".png", ".apng", ".webp", ".jpg", ".jpeg", ".mp4"]
                .iter()
                .any(|suffix| u.path().to_lowercase().ends_with(suffix))
    })
}
struct Frame {
    image: ColorImage,
    delay: Duration,
}
struct Decoded {
    frames: Vec<Frame>,
    duration: Duration,
    bytes: usize,
    fallback: bool,
    sampled: bool,
}
impl Decoded {
    fn new(frames: Vec<Frame>, sampled: bool) -> Result<Self, ()> {
        if frames.is_empty() {
            return Err(());
        }
        let bytes = frames.iter().map(|f| f.image.pixels.len() * 4).sum();
        let duration = frames.iter().map(|f| f.delay).sum();
        Ok(Self {
            frames,
            duration,
            bytes,
            fallback: false,
            sampled,
        })
    }
    fn still(image: ColorImage) -> Self {
        Self::new(
            vec![Frame {
                image,
                delay: Duration::from_secs(1),
            }],
            false,
        )
        .unwrap()
    }
    fn frame_at(&self, elapsed: Duration) -> usize {
        let mut position = elapsed.as_millis() % self.duration.as_millis().max(1);
        for (i, frame) in self.frames.iter().enumerate() {
            if position < frame.delay.as_millis() {
                return i;
            }
            position -= frame.delay.as_millis();
        }
        0
    }
}
fn decode_gif(bytes: &[u8], size: u32) -> Result<Decoded, ()> {
    let mut decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes)).map_err(|_| ())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    decoder.set_limits(limits).map_err(|_| ())?;
    decode_frames(decoder.into_frames(), size)
}
fn decode_frames(
    decoded: impl Iterator<Item = image::ImageResult<image::Frame>>,
    size: u32,
) -> Result<Decoded, ()> {
    let mut frames: Vec<Frame> = Vec::new();
    let mut stride = 1;
    let mut sampled = false;
    let mut edge = size.min(768);
    for (index, frame) in decoded.enumerate() {
        if index >= 1800 {
            return Err(());
        }
        let frame = frame.map_err(|_| ())?;
        let (n, d) = frame.delay().numer_denom_ms();
        let delay = Duration::from_millis((n as u64 / d.max(1) as u64).clamp(10, 10000));
        if index % stride == 0 {
            // Retain the complete cycle's timing while thinning long animations.
            if frames.len() >= FRAME_LIMIT {
                let mut old = std::mem::take(&mut frames).into_iter();
                while let Some(mut frame) = old.next() {
                    if let Some(next) = old.next() {
                        frame.delay += next.delay;
                    }
                    frames.push(frame);
                }
                stride *= 2;
                sampled = true;
            }
            let source = image::DynamicImage::ImageRgba8(frame.into_buffer());
            let image = source.resize(edge.min(source.width()), edge.min(source.height()), image::imageops::FilterType::Triangle).to_rgba8();
            frames.push(Frame {
                image: ColorImage::from_rgba_unmultiplied(
                    [image.width() as usize, image.height() as usize],
                    image.as_raw(),
                ),
                delay,
            });
        } else if let Some(last) = frames.last_mut() {
            last.delay += delay;
        }
        // Reduce resolution before temporal sampling: smooth small decorations stay smooth.
        while frames.iter().map(|f|f.image.pixels.len()*4).sum::<usize>() > ASSET_BUDGET {
            edge = (edge * 3 / 4).max(16);
            shrink_frames(&mut frames, edge);
        }
    }
    Decoded::new(frames, sampled)
}
fn shrink_frames(frames: &mut [Frame], edge: u32) {
    for frame in frames {
        let [w,h]=frame.image.size;
        let rgba:Vec<u8>=frame.image.pixels.iter().flat_map(|p|p.to_array()).collect();
        if let Some(source)=image::RgbaImage::from_raw(w as u32,h as u32,rgba) {
            // Pixels in ColorImage are premultiplied; keep that representation while filtering.
            let resized=image::imageops::resize(&source,((w as f32*edge as f32/w.max(h) as f32).round()as u32).max(1).min(w as u32),((h as f32*edge as f32/w.max(h) as f32).round()as u32).max(1).min(h as u32),image::imageops::FilterType::Triangle);
            frame.image=ColorImage::new([resized.width()as usize,resized.height()as usize],resized.pixels().map(|p|egui::Color32::from_rgba_premultiplied(p[0],p[1],p[2],p[3])).collect());
        }
    }
}
fn decode(bytes: &[u8]) -> Result<Decoded, ()> { decode_sized(bytes, IMAGE_SIZE) }
fn decode_sized(bytes: &[u8], size: u32) -> Result<Decoded, ()> {
    if bytes.len() as u64 > GIF_LIMIT {
        return Err(());
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return decode_gif(bytes, size);
    }
    if bytes.get(4..8)==Some(b"ftyp"){return decode_video(bytes,size);}
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n"){
        let mut decoder=image::codecs::png::PngDecoder::new(Cursor::new(bytes)).map_err(|_|())?;
        let mut limits=image::Limits::default();limits.max_image_width=Some(4096);limits.max_image_height=Some(4096);limits.max_alloc=Some(64*1024*1024);decoder.set_limits(limits).map_err(|_|())?;
        if decoder.is_apng().unwrap_or(false){return decode_frames(decoder.apng().map_err(|_|())?.into_frames(),size);}
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        let mut decoder =
            image::codecs::webp::WebPDecoder::new(Cursor::new(bytes)).map_err(|_| ())?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(64 * 1024 * 1024);
        decoder.set_limits(limits).map_err(|_| ())?;
        if decoder.has_animation() {
            return decode_frames(decoder.into_frames(), size);
        }
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| ())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let source = reader.decode().map_err(|_| ())?;
    let image=source.thumbnail(size.min(source.width()),size.min(source.height())).to_rgba8();
    Ok(Decoded::still(ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    )))
}
fn fetch(client: &reqwest::blocking::Client, url: &str, size: u32) -> Result<Decoded, ()> {
    if !public_url(url) {
        return Err(());
    }
    let response = client
        .get(url)
        .send()
        .map_err(|_| ())?
        .error_for_status()
        .map_err(|_| ())?;
    let limit = GIF_LIMIT;
    if response.content_length().is_some_and(|n| n > limit) {
        return Err(());
    }
    let mut bytes = Vec::new();
    response
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.len() as u64 > limit {
        return Err(());
    }
    decode_sized(&bytes,size)
}
fn download(client: &reqwest::blocking::Client, url: &str) -> Result<Decoded, ()> {
    let (url,size)=source_key(url);
    match fetch(client, url,size) {
        Ok(image) => Ok(image),
        Err(()) if url.contains(".gif?size=256") => {
            let mut image = fetch(client, &url.replace(".gif?size=256", ".png?size=256"),size)?;
            image.fallback = true;
            Ok(image)
        }
        Err(()) => Err(()),
    }
}
#[derive(Default)]
struct Playback {
    elapsed: Duration,
    last_seen: Option<Instant>,
    last_upload: Option<Instant>,
    index: usize,
    uploads: u64,
}
impl Playback {
    fn advance(&mut self, now: Instant, data: &Decoded, interval: Duration) -> Option<usize> {
        if let Some(last) = self.last_seen {
            let delta = now.saturating_duration_since(last);
            if delta <= Duration::from_millis(250) {
                self.elapsed += delta;
            }
        }
        self.last_seen = Some(now);
        let index = data.frame_at(self.elapsed);
        if index != self.index
            && self
                .last_upload
                .is_none_or(|last| now.saturating_duration_since(last) >= interval.saturating_sub(Duration::from_micros(1000)))
        {
            self.index = index;
            self.last_upload = Some(now);
            self.uploads += 1;
            Some(index)
        } else {
            None
        }
    }
}
struct Job {
    generation: u64,
    key: String,
}
struct Ready {
    generation: u64,
    key: String,
    image: Result<Decoded, ()>,
}
struct Texture {
    handle: TextureHandle,
    used: u64,
    data: Decoded,
    playback: Playback,
}
pub struct Images {
    animate:bool,
    frame_interval:Duration,
    tx: Sender<Job>,
    icon_tx: Sender<Job>,
    rx: Receiver<Ready>,
    cancel: Arc<AtomicBool>,
    textures: HashMap<String, Texture>,
    pending: HashSet<String>,
    failed: HashMap<String, Instant>,
    generation: u64,
    clock: u64,
}
impl Images {
    pub fn new(ctx: &egui::Context) -> Self {
        let (tx, jobs) = bounded::<Job>(32);
        let (icon_tx,icon_jobs)=bounded::<Job>(32);
        let (results, rx) = bounded::<Ready>(2);
        let cancel = Arc::new(AtomicBool::new(false));
        for worker in 0..3 {
            let (jobs, results, cancel, ctx) =
                (jobs.clone(), results.clone(), cancel.clone(), ctx.clone());
            let icon_jobs=icon_jobs.clone();
            thread::spawn(move || {
                let client = reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(8))
                    .connect_timeout(Duration::from_secs(5))
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .ok();
                while !cancel.load(Ordering::Relaxed) {
                    let next=if worker==2{icon_jobs.recv_timeout(Duration::from_millis(200))}else{
                        crossbeam_channel::select_biased!{recv(icon_jobs)->job=>job.map_err(|_|crossbeam_channel::RecvTimeoutError::Disconnected),recv(jobs)->job=>job.map_err(|_|crossbeam_channel::RecvTimeoutError::Disconnected),default(Duration::from_millis(200))=>Err(crossbeam_channel::RecvTimeoutError::Timeout)}
                    };
                    let job = match next {
                        Ok(j) => j,
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                        Err(_) => break,
                    };
                    let (source,size)=source_key(&job.key);
                    let image = if let Some(bytes) = bundled(source) {
                        decode_sized(bytes,size)
                    } else if let Some(key) = source.strip_prefix("demo://") {
                        Ok(demo_asset(key))
                    } else {
                        client
                            .as_ref()
                            .ok_or(())
                            .and_then(|c| download(c, &job.key))
                    };
                    let mut ready = Ready {
                        generation: job.generation,
                        key: job.key,
                        image,
                    };
                    loop {
                        if cancel.load(Ordering::Relaxed) {
                            return;
                        }
                        match results.send_timeout(ready, Duration::from_millis(200)) {
                            Ok(()) => {
                                ctx.request_repaint();
                                break;
                            }
                            Err(crossbeam_channel::SendTimeoutError::Timeout(value)) => {
                                ready = value
                            }
                            Err(_) => return,
                        }
                    }
                }
            });
        }
        Self {
            animate:true,frame_interval:FRAME_INTERVAL,
            tx,
            icon_tx,
            rx,
            cancel,
            textures: HashMap::new(),
            pending: HashSet::new(),
            failed: HashMap::new(),
            generation: 0,
            clock: 0,
        }
    }
    pub fn clear(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.textures.clear();
        self.pending.clear();
        self.failed.clear();
    }
    pub fn len(&self) -> usize {
        self.textures.len()
    }
    pub fn bytes(&self) -> usize {
        self.textures.values().map(|v| v.data.bytes).sum()
    }
    pub fn animated(&self) -> usize {
        self.textures
            .values()
            .filter(|v| v.data.frames.len() > 1)
            .count()
    }
    pub fn uploads(&self) -> u64 {
        self.textures.values().map(|v| v.playback.uploads).sum()
    }
    pub fn fallbacks(&self) -> usize {
        self.textures.values().filter(|v| v.data.fallback).count()
    }
    pub fn sampled(&self) -> usize {
        self.textures.values().filter(|v| v.data.sampled).count()
    }
    fn insert(&mut self, ctx: &egui::Context, key: String, data: Decoded) {
        if data.bytes > PIXEL_BUDGET {
            return;
        }
        self.textures.remove(&key);
        // Keep source cadence. Evict least-recently used assets instead of thinning live animations.
        while self.textures.len() >= TEXTURE_LIMIT || self.bytes() + data.bytes > PIXEL_BUDGET {
            let Some(key) = self
                .textures
                .iter()
                .min_by_key(|(_, v)| v.used)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            self.textures.remove(&key);
        }
        self.clock = self.clock.wrapping_add(1);
        let handle = ctx.load_texture(
            &key,
            data.frames[0].image.clone(),
            egui::TextureOptions::LINEAR,
        );
        self.textures.insert(
            key,
            Texture {
                handle,
                used: self.clock,
                data,
                playback: Playback::default(),
            },
        );
    }
    pub fn stress_fill(&mut self, ctx: &egui::Context) {
        self.clear();
        let first = demo_image("stress-animation", 0.0, 64);
        let second = demo_image("stress-animation", 1.0, 64);
        for i in 0..256 {
            let key = format!("demo://cache-workload/{i}");
            let data = if i % 8 == 0 {
                Decoded::new(
                    (0..FRAME_LIMIT)
                        .map(|j| Frame {
                            image: if j % 2 == 0 {
                                first.clone()
                            } else {
                                second.clone()
                            },
                            delay: Duration::from_millis(50),
                        })
                        .collect(),
                    false,
                )
                .unwrap()
            } else {
                Decoded::still(demo_image(&key, 0.0, 128))
            };
            self.insert(ctx, key, data);
        }
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(ready) = self.rx.try_recv() {
            if ready.generation != self.generation {
                continue;
            }
            self.pending.remove(&ready.key);
            match ready.image {
                Ok(data) => self.insert(ctx, ready.key, data),
                Err(()) => {
                    if self.failed.len() >= 256 {
                        if let Some(key) = self
                            .failed
                            .iter()
                            .min_by_key(|(_, v)| **v)
                            .map(|(k, _)| k.clone())
                        {
                            self.failed.remove(&key);
                        }
                    }
                    self.failed.insert(ready.key, Instant::now());
                }
            }
        }
    }
    pub fn texture_sized(&mut self,url:&str,size:egui::Vec2,ctx:&egui::Context)->Option<TextureId>{
        self.texture_key(&cache_key(url,resolution(size,ctx.pixels_per_point())),ctx)
    }
    pub fn dimensions(&self,url:&str,size:egui::Vec2,ctx:&egui::Context)->Option<egui::Vec2>{
        self.textures.get(&cache_key(url,resolution(size,ctx.pixels_per_point()))).map(|t|{let size=t.handle.size();egui::vec2(size[0]as f32,size[1]as f32)})
    }
    pub fn texture(&mut self, key: &str, ctx: &egui::Context) -> Option<TextureId> { self.texture_key(key,ctx) }
    fn texture_key(&mut self, key: &str, ctx: &egui::Context) -> Option<TextureId> {
        self.clock = self.clock.wrapping_add(1);
        if let Some(texture) = self.textures.get_mut(key) {
            texture.used = self.clock;
            if self.animate && texture.data.frames.len() > 1 {
                if let Some(index) = texture.playback.advance(Instant::now(), &texture.data,self.frame_interval) {
                    texture.handle.set(
                        texture.data.frames[index].image.clone(),
                        egui::TextureOptions::LINEAR,
                    );
                }
                ctx.request_repaint_after(self.frame_interval);
            }
            return Some(texture.handle.id());
        }
        if self.pending.len() >= 32
            || self.pending.contains(key)
            || self
                .failed
                .get(key)
                .is_some_and(|t| t.elapsed() < Duration::from_secs(60))
        {
            return None;
        }
        let source=source_key(key).0;
        let queue=if source.contains("/icons/")||source.contains("/avatars/")||source.contains("/embed/avatars/"){&self.icon_tx}else{&self.tx};
        if queue.try_send(Job {
                generation: self.generation,
                key: key.to_owned(),
            })
            .is_ok()
        {
            self.pending.insert(key.to_owned());
        }
        None
    }
    pub fn playback_options(&mut self,animate:bool,fps:u32){self.animate=animate;self.frame_interval=Duration::from_secs_f64(1. / fps.clamp(5,60) as f64);}
    pub fn failed(&self,key:&str)->bool{self.failed.contains_key(key)||[64,128,256,512,1024].into_iter().any(|n|self.failed.contains_key(&cache_key(key,n)))}
}
fn bundled(key:&str)->Option<&'static [u8]>{match key{
    "builtin://discord/nitro-background"=>Some(include_bytes!("../assets/discord/nitro-background.png")),
    "builtin://discord/nitro-wumpus"=>Some(include_bytes!("../assets/discord/nitro-wumpus.webp")),
    "builtin://discord/shop-banner"=>Some(include_bytes!("../assets/discord/shop-banner.png")),
    "builtin://discord/quests-banner"=>Some(include_bytes!("../assets/discord/quests-banner.webp")),
    _=>None,
}}
pub fn animation_check(path:&str)->serde_json::Value {
    let result=std::fs::metadata(path).map_err(|_|()).and_then(|m|if m.len()<=GIF_LIMIT{std::fs::read(path).map_err(|_|())}else{Err(())}).and_then(|bytes|decode(&bytes));
    match result {Ok(image)=>serde_json::json!({"decoded":true,"frames":image.frames.len(),"cycle_ms":image.frames.iter().map(|f|f.delay.as_millis()).sum::<u128>(),"pixel_bytes":image.frames.iter().map(|f|f.image.pixels.len()*4).sum::<usize>(),"first_size":image.frames[0].image.size}),Err(())=>serde_json::json!({"decoded":false})}
}
fn decode_video(bytes:&[u8],size:u32)->Result<Decoded,()>{
    let mut decoder=voice_platform::video::Decoder::open(Box::new(Cursor::new(bytes.to_vec()))).map_err(|_|())?;
    let info=decoder.info();if !info.duration.is_finite()||info.duration>45.||info.width>1920||info.height>1080{return Err(());}
    let mut frames:Vec<Frame>=vec![];let mut edge=size.min(768);let mut stride=1;let mut sampled=false;let mut last_pts=0.;
    for index in 0..1800{let Some(voice_platform::video::Sample::Video{pts,width,height,rgba})=decoder.read_video().map_err(|_|())?else{break;};
        if !pts.is_finite(){return Err(());}if index>0{if let Some(last)=frames.last_mut(){last.delay+=Duration::from_secs_f64((pts-last_pts).clamp(0.001,1.));}}last_pts=pts;
        if index%stride==0{if frames.len()>=FRAME_LIMIT{let mut old=std::mem::take(&mut frames).into_iter();while let Some(mut frame)=old.next(){if let Some(next)=old.next(){frame.delay+=next.delay;}frames.push(frame);}stride*=2;sampled=true;}
            let source=image::RgbaImage::from_raw(width,height,rgba).ok_or(())?;let small=image::DynamicImage::ImageRgba8(source).thumbnail(edge.min(width),edge.min(height)).to_rgba8();frames.push(Frame{image:ColorImage::from_rgba_unmultiplied([small.width()as usize,small.height()as usize],small.as_raw()),delay:Duration::ZERO});}
        while frames.iter().map(|f|f.image.pixels.len()*4).sum::<usize>()>ASSET_BUDGET{edge=(edge*3/4).max(16);shrink_frames(&mut frames,edge);}
    }if let Some(last)=frames.last_mut(){last.delay+=Duration::from_millis(33);}Decoded::new(frames,sampled)
}
impl Drop for Images {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

pub fn network_check() -> serde_json::Value {
    let result = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| ())
        .and_then(|client| {
            download(
                &client,
                "https://cdn.discordapp.com/embed/avatars/0.png?size=128",
            )
        });
    match result {
        Ok(image) => {
            serde_json::json!({"public_cdn":true,"decoded_size":image.frames[0].image.size,"authenticated":false})
        }
        Err(()) => serde_json::json!({"public_cdn":false,"authenticated":false}),
    }
}

// Offline preview artwork is generated locally; live accounts use actual CDN images.
fn demo_asset(key: &str) -> Decoded {
    if key.contains("Alex") || key.contains("local-night") || key.contains("animated") {
        Decoded::new(
            (0..20)
                .map(|i| Frame {
                    image: demo_image(key, i as f32 * std::f32::consts::TAU / 20.0, 64),
                    delay: Duration::from_millis(60),
                })
                .collect(),
            false,
        )
        .unwrap()
    } else {
        Decoded::still(demo_image(key, 0.0, 128))
    }
}
fn demo_image(key: &str, phase: f32, size: usize) -> ColorImage {
    let seed = key
        .bytes()
        .fold(37u32, |n, b| n.wrapping_mul(31).wrapping_add(b as u32));
    let palette = [
        [76, 179, 190],
        [150, 115, 212],
        [228, 153, 113],
        [112, 177, 140],
        [217, 126, 166],
        [126, 158, 217],
    ];
    let color = palette[seed as usize % palette.len()];
    let mut bytes = vec![0; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let xf = x as f32 * 128.0 / size as f32 - 64.0;
            let yf = y as f32 * 128.0 / size as f32 - 64.0 + phase.sin() * 5.0;
            let face = xf * xf / (44.0 * 44.0) + (yf + 1.0) * (yf + 1.0) / (40.0 * 40.0) < 1.0;
            let eye = ((xf - 17.0).powi(2) + (yf + 5.0).powi(2) < 20.0)
                || ((xf + 17.0).powi(2) + (yf + 5.0).powi(2) < 20.0);
            let smile =
                yf > 10.0 && yf < 22.0 && xf.abs() < 17.0 && xf * xf + (yf - 9.0).powi(2) < 270.0;
            let ears = (xf.abs() - 31.0).powi(2) + (yf + 31.0).powi(2) < 200.0;
            let c = if eye || smile {
                [26, 32, 46]
            } else if face || ears {
                color
            } else {
                [
                    24 + (x * 128 / size / 7) as u8,
                    28 + (y * 128 / size / 8) as u8,
                    43 + (x * 128 / size / 10) as u8,
                ]
            };
            let i = (y * size + x) * 4;
            bytes[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    ColorImage::from_rgba_unmultiplied([size, size], &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn sixty_fps_playback_does_not_retain_the_old_thirty_fps_upload_gate(){
        let data=Decoded::new((0..60).map(|_|Frame{image:ColorImage::new([1,1],vec![egui::Color32::WHITE]),delay:Duration::from_millis(17)}).collect(),false).unwrap();
        let start=Instant::now();let mut playback=Playback::default();playback.advance(start,&data,FRAME_INTERVAL);
        for i in 1..=30{assert!(playback.advance(start+Duration::from_millis(i*17),&data,FRAME_INTERVAL).is_some());}
        assert_eq!(playback.uploads,30);
        let mut cache=Images::new(&egui::Context::default());cache.playback_options(true,60);assert!(cache.frame_interval<Duration::from_millis(17));
    }
    #[test]fn display_sizing_keeps_large_art_sharp_and_animation_resolution_bounded(){
        assert_eq!(resolution(egui::vec2(40.,40.),2.),128);assert_eq!(resolution(egui::vec2(360.,150.),2.),1024);
        let mut bytes=Cursor::new(Vec::new());image::DynamicImage::new_rgba8(1600,600).write_to(&mut bytes,image::ImageFormat::Png).unwrap();
        let large=decode_sized(bytes.get_ref(),1024).unwrap();assert_eq!(large.frames[0].image.size,[1024,384]);
        let small=decode_sized(bytes.get_ref(),128).unwrap();assert_eq!(small.frames[0].image.size,[128,48]);
        let frames=(0..80).map(|_|Ok(image::Frame::from_parts(image::RgbaImage::from_pixel(512,128,image::Rgba([20,40,60,255])),0,0,image::Delay::from_numer_denom_ms(17,1))));
        let animation=decode_frames(frames,512).unwrap();assert_eq!(animation.frames.len(),80);assert_eq!(animation.duration,Duration::from_millis(1360));assert!(animation.bytes<=ASSET_BUDGET);assert!(!animation.sampled);assert!(animation.frames[0].image.size[0]>128);
    }
    #[test]
    fn builds_cdn_urls_and_selects_modern_legacy_and_guild_avatars() {
        let mut user = User {
            id: "1152921504606846977".into(),
            ..Default::default()
        };
        assert!(avatar_url(&user, None, None)
            .unwrap()
            .contains("/embed/avatars/4.png"));
        user.discriminator = "1234".into();
        assert!(avatar_url(&user, None, None)
            .unwrap()
            .contains("/embed/avatars/4.png"));
        user.avatar = Some("a_0123456789abcdef0123456789abcdef".into());
        assert!(avatar_url(&user, None, None)
            .unwrap()
            .ends_with(".gif?size=256"));
        let guild = Guild {
            id: "42".into(),
            icon: user.avatar.clone(),
            ..Default::default()
        };
        assert!(guild_url(&guild).unwrap().ends_with(".gif?size=256"));
        assert!(avatar_url(&user, None, None)
            .unwrap()
            .contains("/avatars/1152921504606846977/a_"));
        assert!(
            avatar_url(&user, Some("42"), Some("abcdef0123456789abcdef0123456789"))
                .unwrap()
                .contains("/guilds/42/users/")
        );
        assert!(!public_url("https://cdn.discordapp.com.evil.com/a.png"));
        assert!(!public_url("http://cdn.discordapp.com/a.png"));
        user.id = "../bad".into();
        assert!(avatar_url(&user, None, None).is_none());
    }
    #[test]
    fn rejects_oversized_and_broken_images_and_bounds_decoded_pixels() {
        assert!(decode(b"not a png").is_err());
        let mut buffer = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(4097, 10)
            .write_to(&mut buffer, image::ImageFormat::Png)
            .unwrap();
        assert!(decode(buffer.get_ref()).is_err());
        let mut buffer = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(256, 256)
            .write_to(&mut buffer, image::ImageFormat::Png)
            .unwrap();
        let image = decode(buffer.get_ref()).unwrap();
        assert_eq!(image.frames[0].image.size, [256, 256]);
    }
    #[test]
    fn decodes_gif_timing_loops_thins_long_cycles_and_pauses_offscreen() {
        fn gif(count: usize) -> Vec<u8> {
            let mut bytes = Vec::new();
            {
                let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
                for i in 0..count {
                    let buffer = image::RgbaImage::from_pixel(
                        8,
                        8,
                        image::Rgba([
                            if i % 2 == 0 { 255 } else { 0 },
                            0,
                            if i % 2 == 0 { 0 } else { 255 },
                            255,
                        ]),
                    );
                    encoder
                        .encode_frame(image::Frame::from_parts(
                            buffer,
                            0,
                            0,
                            image::Delay::from_numer_denom_ms(100, 1),
                        ))
                        .unwrap();
                }
            }
            bytes
        }
        let data = decode(&gif(2)).unwrap();
        assert_eq!(data.frames.len(), 2);
        assert_eq!(data.duration, Duration::from_millis(200));
        assert_ne!(data.frames[0].image.pixels, data.frames[1].image.pixels);
        let start = Instant::now();
        let mut playback = Playback::default();
        assert_eq!(playback.advance(start, &data,FRAME_INTERVAL), None);
        assert_eq!(
            playback.advance(start + Duration::from_millis(110), &data,FRAME_INTERVAL),
            Some(1)
        );
        assert_eq!(
            playback.advance(start + Duration::from_secs(3), &data,FRAME_INTERVAL),
            None
        );
        assert_eq!(playback.elapsed, Duration::from_millis(110));
        assert_eq!(
            playback.advance(start + Duration::from_millis(3110), &data,FRAME_INTERVAL),
            Some(0)
        );
        let long = decode(&gif(750)).unwrap();
        assert!(long.frames.len() <= FRAME_LIMIT);
        assert_eq!(long.duration, Duration::from_secs(75));
        assert!(long.sampled);
    }
    #[test]
    fn cache_evicts_old_images_and_discards_inflight_images_after_clear() {
        let ctx = egui::Context::default();
        let mut cache = Images::new(&ctx);
        for i in 0..160 {
            let key = format!("demo://cache/{i}");
            let until = Instant::now() + Duration::from_secs(5);
            while cache.texture(&key, &ctx).is_none() {
                cache.poll(&ctx);
                assert!(Instant::now() < until, "image worker did not finish");
                thread::sleep(Duration::from_millis(1));
            }
            assert!(cache.len() <= TEXTURE_LIMIT);
            assert!(cache.pending.len() <= 32);
        }
        assert_eq!(cache.len(), TEXTURE_LIMIT);
        assert!(!cache.textures.contains_key("demo://cache/0"));
        cache.stress_fill(&ctx);
        assert!(cache.bytes() <= PIXEL_BUDGET);
        assert!(cache.bytes() > PIXEL_BUDGET - ASSET_BUDGET);
        assert!(cache.animated() > 0);
        cache.clear();
        for i in 0..24 {
            let data = Decoded::new(
                (0..120)
                    .map(|_| Frame {
                        image: demo_image("memory", 0.0, 64),
                        delay: Duration::from_millis(50),
                    })
                    .collect(),
                false,
            )
            .unwrap();
            cache.insert(&ctx, format!("animation-{i}"), data);
        }
        assert!(cache.len() <= 24);
        assert!(cache.animated() > 0);
        assert!(cache.bytes() <= PIXEL_BUDGET);
        assert!(cache
            .textures
            .values()
            .all(|v| v.data.duration == Duration::from_secs(6)));
        cache.texture("demo://inflight", &ctx);
        cache.clear();
        thread::sleep(Duration::from_millis(30));
        cache.poll(&ctx);
        assert_eq!(cache.len(), 0);
        assert!(cache.pending.is_empty());
    }
}
