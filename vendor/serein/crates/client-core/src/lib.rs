pub mod screen;
pub mod voice {
use model::Id;
use zeroize::Zeroizing;
pub const MAX_PARTICIPANTS:usize=64;
pub struct Secret(Zeroizing<String>);
impl Secret {
pub fn new(s:String)->Result<Self,&'static str>{if s.is_empty()||s.len()>2048||!s.bytes().all(|c|c.is_ascii_graphic()){return Err("Invalid voice credential")}Ok(Self(Zeroizing::new(s)))}
pub fn expose(&self)->&str{&self.0}
}
pub struct VoiceConnection {pub channel:Id,pub guild:Option<Id>,pub user:Id,pub peer:Option<Id>,pub session:Secret,pub token:Secret,pub endpoint:String,pub request:u64}
}
