//! Record on release so a modifier can be either a key or part of a chord.
use crate::preferences::Preferences;
use std::time::{Duration, Instant};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Binding { pub key: u32, pub ctrl: bool, pub shift: bool, pub alt: bool }
pub struct Recorder { started: Instant, armed: bool, candidate: Option<Binding> }
impl Recorder {
    pub fn new() -> Self { Self { started: Instant::now(), armed: false, candidate: None } }
    pub fn expired(&self) -> bool { self.started.elapsed() > Duration::from_secs(15) }
    pub fn poll(&mut self, down: impl Fn(i32) -> bool) -> Option<Binding> {
        // Wait for the initiating click and any held shortcut to be released.
        let key = (1..=254).filter(|k| !matches!(k,16|17|18|160..=165)).find(|k| down(*k));
        let (ctrl,shift,alt)=(down(17),down(16),down(18));
        let held=key.is_some()||ctrl||shift||alt;
        if !self.armed { if !held { self.armed=true; } return None; }
        if !held { return self.candidate.take(); }
        let binding=if let Some(key)=key { Binding{key:key as u32,ctrl,shift,alt} }
            else if alt { Binding{key:18,ctrl,shift,alt:false} }
            else if shift { Binding{key:16,ctrl,shift:false,alt:false} }
            else { Binding{key:17,ctrl:false,shift:false,alt:false} };
        // Keep the fullest chord while its keys are released one at a time.
        let weight=|b:Binding| (!matches!(b.key,16..=18)) as u8*4+b.ctrl as u8+b.shift as u8+b.alt as u8;
        if self.candidate.is_none_or(|old|weight(binding)>=weight(old)) { self.candidate=Some(binding); }
        None
    }
}
impl Binding { pub fn apply(self,p:&mut Preferences) { p.ptt_key=self.key;p.ptt_ctrl=self.ctrl;p.ptt_shift=self.shift;p.ptt_alt=self.alt; } }
#[cfg(test)] mod tests {
    use super::*;
    fn step(r:&mut Recorder,keys:&[i32])->Option<Binding>{r.poll(|k|keys.contains(&k))}
    #[test] fn records_standalone_modifiers_and_chords_on_release(){
        for key in [13,16,17,18,32,119,5] {let mut r=Recorder::new();step(&mut r,&[]);assert_eq!(step(&mut r,&[key]),None);assert_eq!(step(&mut r,&[]).unwrap().key,key as u32);}
        let mut r=Recorder::new();step(&mut r,&[1]);step(&mut r,&[]);step(&mut r,&[17]);step(&mut r,&[17,16,18,65]);step(&mut r,&[17,16]);step(&mut r,&[17]);
        assert_eq!(step(&mut r,&[]),Some(Binding{key:65,ctrl:true,shift:true,alt:true}));
        let mut r=Recorder::new();step(&mut r,&[]);step(&mut r,&[17,16]);step(&mut r,&[17]);assert_eq!(step(&mut r,&[]),Some(Binding{key:16,ctrl:true,shift:false,alt:false}));
    }
}
