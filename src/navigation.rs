//! Small session-only navigation cache. Gateway invalidation prevents stale revisits.
use crate::model::{Channel,Message};
use std::collections::VecDeque;
const HISTORY_COUNT:usize=6;
const HISTORY_BYTES:usize=2*1024*1024;
#[derive(Default)] pub struct Cache {
    histories:VecDeque<(String,VecDeque<Message>,bool,usize)>,
    guilds:VecDeque<(String,Vec<Channel>)>,
    selected:VecDeque<(String,String)>,
}
impl Cache {
    pub fn select(&mut self,channel:&Channel){if let Some(guild)=&channel.guild_id{self.selected.retain(|(id,_)|id!=guild);if self.selected.len()>=24{self.selected.pop_front();}self.selected.push_back((guild.clone(),channel.id.clone()));}}
    pub fn last_channel(&self,guild:&str)->Option<String>{self.selected.iter().find(|(id,_)|id==guild).map(|(_,id)|id.clone())}
    pub fn save(&mut self,id:&str,messages:&VecDeque<Message>,older:bool){
        self.invalidate(id);
        let older=older||messages.len()>80;
        let messages:VecDeque<_>=messages.iter().rev().take(80).cloned().collect::<Vec<_>>().into_iter().rev().collect();
        let bytes=serde_json::to_vec(&messages).map(|v|v.len()).unwrap_or(HISTORY_BYTES+1);
        if bytes>HISTORY_BYTES{return;}
        while self.histories.len()>=HISTORY_COUNT||self.histories.iter().map(|e|e.3).sum::<usize>()+bytes>HISTORY_BYTES{self.histories.pop_front();}
        self.histories.push_back((id.into(),messages,older,bytes));
    }
    pub fn get(&mut self,id:&str)->Option<(VecDeque<Message>,bool)>{let index=self.histories.iter().position(|e|e.0==id)?;let entry=self.histories.remove(index)?;let value=(entry.1.clone(),entry.2);self.histories.push_back(entry);Some(value)}
    pub fn invalidate(&mut self,id:&str){self.histories.retain(|e|e.0!=id);}
    pub fn receive(&mut self,m:Message){if let Some(index)=self.histories.iter().position(|e|e.0==m.channel_id){let(id,mut messages,older,_)=self.histories.remove(index).unwrap();crate::model::merge_message(&mut messages,m);self.save(&id,&messages,older);}}
    pub fn save_channels(&mut self,id:&str,channels:&[Channel]){self.invalidate_channels(id);if channels.len()>500{return;}while self.guilds.len()>=12||self.guilds.iter().map(|e|e.1.len()).sum::<usize>()+channels.len()>2000{self.guilds.pop_front();}self.guilds.push_back((id.into(),channels.to_vec()));}
    pub fn channels(&mut self,id:&str)->Option<Vec<Channel>>{let index=self.guilds.iter().position(|e|e.0==id)?;let entry=self.guilds.remove(index)?;let value=entry.1.clone();self.guilds.push_back(entry);Some(value)}
    pub fn invalidate_channels(&mut self,id:&str){self.guilds.retain(|e|e.0!=id);}
}
pub fn sort_channels(channels:&mut [Channel]){let positions:std::collections::HashMap<_,_>=channels.iter().filter(|c|c.kind==4).map(|c|(c.id.clone(),c.position)).collect();channels.sort_by_key(|c|(c.parent_id.as_ref().and_then(|id|positions.get(id)).copied().unwrap_or(c.position),if c.kind==4{-1}else{c.position}));}
#[cfg(test)]mod tests{use super::*;
    #[test]fn revisits_keep_latest_messages_and_invalidation_drops_stale_history(){let mut c=Cache::default();let m=Message{id:"1".into(),channel_id:"chat".into(),content:"before".into(),..Default::default()};c.save("chat",&VecDeque::from([m.clone()]),true);let mut edited=m;edited.content="after".into();c.receive(edited);assert_eq!(c.get("chat").unwrap().0[0].content,"after");c.invalidate("chat");assert!(c.get("chat").is_none());}
    #[test]fn caches_are_bounded_and_guilds_stay_separate(){let mut c=Cache::default();for n in 0..30{c.save(&n.to_string(),&VecDeque::new(),false);c.save_channels(&n.to_string(),&[Channel{id:n.to_string(),..Default::default()}]);}assert_eq!(c.histories.len(),6);assert_eq!(c.guilds.len(),12);assert!(c.get("0").is_none());assert_eq!(c.channels("29").unwrap()[0].id,"29");c.invalidate_channels("29");assert!(c.channels("29").is_none());}
}
