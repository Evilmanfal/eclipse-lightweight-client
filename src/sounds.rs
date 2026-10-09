//! Short native feedback sounds. Static WAV buffers remain alive for async WinMM playback.
#[derive(Clone,Copy,Debug,PartialEq)] pub enum Cue {Mute,Unmute,Deafen,Undeafen,Join,Leave,PttOn,PttOff}
fn wav(cue:Cue)->&'static [u8]{match cue{
    Cue::Join=>include_bytes!("../assets/sounds/join.wav"),
    Cue::Leave=>include_bytes!("../assets/sounds/leave.wav"),
    Cue::Mute=>include_bytes!("../assets/sounds/mute.wav"),
    Cue::Unmute=>include_bytes!("../assets/sounds/unmute.wav"),
    Cue::Deafen=>include_bytes!("../assets/sounds/deafen.wav"),
    Cue::Undeafen=>include_bytes!("../assets/sounds/undeafen.wav"),
    Cue::PttOn=>{static ON:std::sync::OnceLock<Vec<u8>>=std::sync::OnceLock::new();ON.get_or_init(||blip(560.,820.)).as_slice()}
    Cue::PttOff=>{static OFF:std::sync::OnceLock<Vec<u8>>=std::sync::OnceLock::new();OFF.get_or_init(||blip(820.,560.)).as_slice()}
}}
/// A soft 70 ms sine chirp for push to talk (rising when the microphone opens, falling when it closes).
fn blip(from:f32,to:f32)->Vec<u8>{
    const RATE:u32=44_100;let samples=RATE*70/1000;
    let mut data=Vec::with_capacity(44+samples as usize*2);
    let push32=|d:&mut Vec<u8>,v:u32|d.extend_from_slice(&v.to_le_bytes());
    data.extend_from_slice(b"RIFF");push32(&mut data,36+samples*2);data.extend_from_slice(b"WAVEfmt ");
    push32(&mut data,16);data.extend_from_slice(&1u16.to_le_bytes());data.extend_from_slice(&1u16.to_le_bytes());push32(&mut data,RATE);push32(&mut data,RATE*2);data.extend_from_slice(&2u16.to_le_bytes());data.extend_from_slice(&16u16.to_le_bytes());
    data.extend_from_slice(b"data");push32(&mut data,samples*2);
    let mut phase=0f32;
    for i in 0..samples{let t=i as f32/samples as f32;phase+=std::f32::consts::TAU*(from+(to-from)*t)/RATE as f32;
        let envelope=(t*14.).min(1.)*(1.-t).powf(1.6);
        data.extend_from_slice(&((phase.sin()*envelope*0.22*i16::MAX as f32) as i16).to_le_bytes());}
    data
}
pub fn play(cue:Cue,enabled:bool){
    if !enabled{return;}
    #[cfg(test)] PLAYED.with(|played|played.borrow_mut().push(cue));
    #[cfg(all(windows,not(test)))] unsafe {
        #[link(name="winmm")] unsafe extern "system"{fn PlaySoundW(sound:*const u16,module:*mut core::ffi::c_void,flags:u32)->i32;}
        // SND_ASYNC | SND_MEMORY | SND_NODEFAULT. No device capture or audio service.
        PlaySoundW(wav(cue).as_ptr().cast(),core::ptr::null_mut(),0x0001|0x0004|0x0002);
    }
    #[cfg(any(not(windows),test))] let _=wav(cue);
}
#[cfg(test)] mod tests{use super::*;
    #[test]fn six_feedback_cues_are_distinct_bounded_pcm(){let cues=[Cue::Mute,Cue::Unmute,Cue::Deafen,Cue::Undeafen,Cue::Join,Cue::Leave,Cue::PttOn,Cue::PttOff];for(i,cue)in cues.iter().enumerate(){let data=wav(*cue);assert_eq!(&data[..4],b"RIFF");assert_eq!(&data[8..12],b"WAVE");assert!(data.len()<32_000);for other in &cues[..i]{assert_ne!(data,wav(*other));}}}
}

#[cfg(test)]std::thread_local!{static PLAYED:std::cell::RefCell<Vec<Cue>>=const{std::cell::RefCell::new(Vec::new())};}
#[cfg(test)]pub fn take_played()->Vec<Cue>{PLAYED.with(|played|std::mem::take(&mut *played.borrow_mut()))}
