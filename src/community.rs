//! Bounded member and role state for the selected server. Unknown permissions fail closed.
use crate::model::User;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

pub const MEMBER_LIMIT: usize = 1000;
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Role {
    pub id: String,
    #[serde(default)] pub name: String,
    #[serde(default)] pub color: u32,
    #[serde(default)] pub colors: Option<Value>,
    #[serde(default)] pub position: i32,
    #[serde(default)] pub permissions: String,
    #[serde(default)] pub hoist: bool,
    #[serde(default)] pub mentionable: bool,
    #[serde(default)] pub managed: bool,
}
impl Role {
    pub fn bits(&self) -> u64 { self.permissions.parse().unwrap_or(0) }
    pub fn rgb(&self) -> u32 { self.colors.as_ref().and_then(|v|v["primary_color"].as_u64()).map(|v|v as u32).unwrap_or(self.color) }
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Member {
    #[serde(default)] pub user: User,
    pub nick: Option<String>,
    pub avatar: Option<String>,
    pub avatar_decoration_data: Option<Value>,
    #[serde(default)] pub roles: Vec<String>,
    #[serde(default)] pub mute: bool,
    #[serde(default)] pub deaf: bool,
}
impl Member { pub fn name(&self)->&str { self.nick.as_deref().unwrap_or_else(||self.user.name()) } }
#[derive(Default)]
pub struct Server {
    pub id: String,
    pub metadata: Value,
    pub roles: Vec<Role>,
    pub members: HashMap<String, Member>,
    pub has_more: bool,
    pub cursor: Option<String>,
    pub online_count: Option<u64>,
}
impl Server {
    pub fn select(&mut self,id:&str) { if self.id!=id {*self=Self {id:id.into(),..Default::default()};} }
    pub fn insert(&mut self,member:Member) { if !member.user.id.is_empty() && (self.members.contains_key(&member.user.id)||self.members.len()<MEMBER_LIMIT) { self.members.insert(member.user.id.clone(),member); } }
    pub fn top(&self, member:&Member)->Option<&Role> { self.roles.iter().filter(|r|member.roles.contains(&r.id)).max_by_key(|r|r.position) }
    pub fn color(&self,id:&str)->Option<eframe::egui::Color32> {
        let member=self.members.get(id)?;
        let role=self.roles.iter().filter(|r|member.roles.contains(&r.id)&&r.rgb()!=0).max_by_key(|r|r.position)?;
        let c=role.rgb();Some(eframe::egui::Color32::from_rgb((c>>16)as u8,(c>>8)as u8,c as u8))
    }
    pub fn permissions(&self,user:&str)->u64 {
        if self.metadata["owner_id"].as_str()==Some(user) {return u64::MAX;}
        let Some(member)=self.members.get(user)else{return 0;};
        let bits=self.roles.iter().filter(|r|r.id==self.id||member.roles.contains(&r.id)).fold(0,|bits,r|bits|r.bits());
        if bits&8!=0 {u64::MAX}else{bits}
    }
    pub fn can(&self,user:&str,bit:u8)->bool { self.permissions(user)&(1u64<<bit)!=0 }
    pub fn can_manage(&self,actor:&str,target:&str)->bool {
        if actor==target||self.metadata["owner_id"].as_str()==Some(target) {return false;}
        if self.metadata["owner_id"].as_str()==Some(actor) {return true;}
        let rank=|id:&str|self.members.get(id).and_then(|m|self.top(m)).map(|r|r.position).unwrap_or(0);
        self.members.contains_key(target)&&rank(actor)>rank(target)
    }
    pub fn can_edit_role(&self,actor:&str,role:&Role)->bool {
        if role.managed||!self.can(actor,28) {return false;}
        self.metadata["owner_id"].as_str()==Some(actor)||self.members.get(actor).and_then(|m|self.top(m)).is_some_and(|r|r.position>role.position)
    }
    pub fn ingest(&mut self,kind:&str,data:&Value) {
        if kind=="READY" { if let Some(gs)=data["guilds"].as_array(){for g in gs {self.ingest("GUILD_CREATE",g);}} return; }
        let guild=data["guild_id"].as_str().or_else(||data["id"].as_str()).unwrap_or("");
        if guild!=self.id||self.id.is_empty(){return;}
        if let Some(count)=data["online_count"].as_u64(){self.online_count=Some(count);}
        match kind {
            "GUILD_CREATE"|"GUILD_UPDATE"=>{
                if self.metadata.is_null(){self.metadata=serde_json::json!({});}
                if let Some(obj)=data.as_object(){for key in ["id","name","owner_id","description","approximate_member_count","member_count"] {if let Some(v)=obj.get(key){self.metadata[key]=v.clone();}}}
                if let Some(roles)=data.get("roles"){if let Ok(roles)=serde_json::from_value(roles.clone()){self.roles=roles;}}
            }
            "GUILD_ROLE_CREATE"|"GUILD_ROLE_UPDATE"=>{if let Ok(role)=serde_json::from_value::<Role>(data["role"].clone()){self.roles.retain(|r|r.id!=role.id);self.roles.push(role);}}
            "GUILD_ROLE_DELETE"=>self.roles.retain(|r|Some(r.id.as_str())!=data["role_id"].as_str()),
            "GUILD_MEMBER_REMOVE"=>{if let Some(id)=data["user"]["id"].as_str(){self.members.remove(id);}},
            "GUILD_MEMBER_UPDATE"|"GUILD_MEMBER_ADD"=>self.member_value(data),
            _=>{}
        }
        if let Some(members)=data["members"].as_array(){for m in members.iter().take(MEMBER_LIMIT){self.member_value(m);}}
        if let Some(ops)=data["ops"].as_array(){for op in ops {if let Some(items)=op["items"].as_array(){for item in items.iter().take(MEMBER_LIMIT){self.member_value(&item["member"]);}}self.member_value(&op["item"]["member"]);}}
    }
    fn member_value(&mut self,v:&Value){if let Ok(mut member)=serde_json::from_value::<Member>(v.clone()){
        if let Some(old)=self.members.get(&member.user.id){if v.get("nick").is_none(){member.nick=old.nick.clone();}if v.get("avatar").is_none(){member.avatar=old.avatar.clone();}if v.get("avatar_decoration_data").is_none(){member.avatar_decoration_data=old.avatar_decoration_data.clone();}if v.get("roles").is_none(){member.roles=old.roles.clone();}member.user=crate::identity::merged(&old.user,&serde_json::json!({"user":v["user"]}));}
        self.insert(member);
    }}
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Relationship {
    pub id:String,
    #[serde(rename="type",default)]pub kind:u8,
    #[serde(default)]pub user:User,
    pub nickname:Option<String>,
}
pub fn snowflake(id:&str)->bool { !id.is_empty()&&id.len()<=20&&id.bytes().all(|b|b.is_ascii_digit())&&id.parse::<u64>().is_ok_and(|n|n>0) }
/// Strip private tokens from optional API responses before handing data to UI state.
pub fn scrub(v:&mut Value){match v {Value::Object(o)=>{o.retain(|k,_|!matches!(k.as_str(),"token"|"access_token"|"refresh_token"|"session_id_hash"|"password"|"email"|"phone"));for v in o.values_mut(){scrub(v);}},Value::Array(a)=>{for v in a {scrub(v);}},_=>{}}}
#[cfg(test)]mod tests {use super::*;use serde_json::json;
#[test]fn permission_hierarchy_and_unknown_members(){let mut s=Server{id:"10".into(),metadata:json!({"owner_id":"1"}),..Default::default()};s.roles=serde_json::from_value(json!([{"id":"10","permissions":"1024"},{"id":"20","position":5,"permissions":"268435458"},{"id":"21","position":2}])).unwrap();s.ingest("GUILD_MEMBERS_CHUNK",&json!({"guild_id":"10","members":[{"user":{"id":"2"},"roles":["20"]},{"user":{"id":"3"},"roles":["21"]}]}));assert!(s.can("2",28));assert!(s.can_manage("2","3"));assert!(!s.can_manage("3","2"));assert!(!s.can("999",28));assert!(!s.can_manage("2","1"));assert!(s.can("1",28));}
#[test]fn member_events_are_scoped_and_bounded(){let mut s=Server{id:"10".into(),..Default::default()};s.ingest("GUILD_MEMBER_ADD",&json!({"guild_id":"11","user":{"id":"2"}}));assert!(s.members.is_empty());for n in 1..=1100{s.insert(Member{user:User{id:n.to_string(),..Default::default()},..Default::default()});}assert_eq!(s.members.len(),MEMBER_LIMIT);}
#[test]fn secrets_are_removed_recursively(){let mut v=json!({"token":"secret","nested":[{"access_token":"secret","name":"ok"}]});scrub(&mut v);assert_eq!(v,json!({"nested":[{"name":"ok"}]}));assert!(!snowflake("1/../2"));}
}
