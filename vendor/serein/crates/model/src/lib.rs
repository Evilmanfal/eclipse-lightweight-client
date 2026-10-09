pub mod voice_settings;
#[derive(Clone,Copy,Debug,PartialEq,Eq,Hash)]
pub struct Id(pub u64);
impl std::fmt::Display for Id {fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{self.0.fmt(f)}}
