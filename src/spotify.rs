//! Spotify controls through Windows media sessions. No Spotify/Discord tokens or injection.
use eframe::egui;
use crossbeam_channel::{bounded,Sender,Receiver};
use std::{sync::{Arc,atomic::{AtomicBool,Ordering}},time::Duration};
use windows::Media::Control::{GlobalSystemMediaTransportControlsSessionManager as Manager,GlobalSystemMediaTransportControlsSessionPlaybackStatus as Playback};
#[derive(Clone,Default)]pub struct Track{pub title:String,pub artist:String,pub album:String,pub playing:bool,pub shuffle:bool,pub repeat:bool,pub position:f64,pub duration:f64,pub available:bool,pub error:Option<String>}
pub enum Action{Previous,Toggle,Next,Seek(f64),Shuffle(bool),Repeat(bool)}
pub struct Spotify{pub track:Track,tx:Sender<Action>,rx:Receiver<Track>,cancel:Arc<AtomicBool>}
impl Spotify{
pub fn new(ctx:egui::Context)->Self{
 let(tx,commands)=bounded(8);let(events,rx)=bounded(1);let cancel=Arc::new(AtomicBool::new(false));let stop=cancel.clone();
 std::thread::spawn(move||{
  unsafe{let _=windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);}
  let manager=Manager::RequestAsync().and_then(|o|o.get()).ok();
  while !stop.load(Ordering::Relaxed){
   let mut track=Track::default();
   let session=manager.as_ref().and_then(|m|m.GetSessions().ok()).and_then(|sessions|sessions.into_iter().find(|s|s.SourceAppUserModelId().is_ok_and(|id|id.to_string().to_ascii_lowercase().contains("spotify"))));
   if let Some(session)=session {
    for action in commands.try_iter().take(8){let result=match action {Action::Previous=>session.TrySkipPreviousAsync().and_then(|o|o.get()),Action::Next=>session.TrySkipNextAsync().and_then(|o|o.get()),Action::Toggle=>session.TryTogglePlayPauseAsync().and_then(|o|o.get()),Action::Seek(seconds)=>session.TryChangePlaybackPositionAsync((seconds.max(0.0)*10_000_000.0)as i64).and_then(|o|o.get()),Action::Shuffle(on)=>session.TryChangeShuffleActiveAsync(on).and_then(|o|o.get()),Action::Repeat(on)=>session.TryChangeAutoRepeatModeAsync(if on {windows::Media::MediaPlaybackAutoRepeatMode::Track}else{windows::Media::MediaPlaybackAutoRepeatMode::None}).and_then(|o|o.get())};if !matches!(result,Ok(true)){track.error=Some("Spotify did not accept that playback command.".into());}}
    if let Ok(props)=session.TryGetMediaPropertiesAsync().and_then(|o|o.get()){track.title=props.Title().map(|v|v.to_string()).unwrap_or_default();track.artist=props.Artist().map(|v|v.to_string()).unwrap_or_default();track.album=props.AlbumTitle().map(|v|v.to_string()).unwrap_or_default();track.available=true;}
    if let Ok(info)=session.GetPlaybackInfo(){track.playing=info.PlaybackStatus().is_ok_and(|s|s==Playback::Playing);track.shuffle=info.IsShuffleActive().and_then(|v|v.Value()).unwrap_or(false);track.repeat=info.AutoRepeatMode().and_then(|v|v.Value()).is_ok_and(|v|v==windows::Media::MediaPlaybackAutoRepeatMode::Track);}
    if let Ok(t)=session.GetTimelineProperties(){track.position=t.Position().map(|t|t.Duration as f64/10_000_000.0).unwrap_or(0.);track.duration=t.EndTime().map(|t|t.Duration as f64/10_000_000.0).unwrap_or(0.);}
   }else{while commands.try_recv().is_ok(){}}
   if events.try_send(track).is_ok(){ctx.request_repaint();}
   for _ in 0..10{if stop.load(Ordering::Relaxed){return;}std::thread::sleep(Duration::from_millis(100));}
  }
 });Self{track:Track::default(),tx,rx,cancel}
}
pub fn poll(&mut self){if let Ok(track)=self.rx.try_recv(){self.track=track;}}
pub fn send(&self,action:Action){let _=self.tx.try_send(action);}
}
impl Drop for Spotify{fn drop(&mut self){self.cancel.store(true,Ordering::Relaxed);}}
