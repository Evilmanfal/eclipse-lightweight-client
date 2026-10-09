//! Who is in which voice channel, for every server and DM call the account can see, whether or not
//! Eclipse itself has joined. Fed by READY / GUILD_CREATE snapshots and live VOICE_STATE_UPDATEs.
use crate::model::User;
use serde_json::Value;
use std::collections::HashMap;

const LIMIT: usize = 20_000;

#[derive(Clone, Debug, Default)]
pub struct VoiceState {
    pub user_id: String,
    pub guild_id: Option<String>,
    pub channel_id: String,
    pub muted: bool,
    pub deafened: bool,
    pub video: bool,
    pub streaming: bool,
    /// Present when Discord attached the member to the state.
    pub user: Option<User>,
    pub nick: Option<String>,
}

#[derive(Default)]
pub struct VoiceRoster {
    /// Keyed by (guild id or "" for DM calls, user id); a user has one voice state per scope.
    states: HashMap<(String, String), VoiceState>,
}

impl VoiceRoster {
    pub fn ingest(&mut self, kind: &str, data: &Value) {
        match kind {
            "READY" => {
                for guild in data["guilds"].as_array().into_iter().flatten().take(1000) {
                    self.snapshot(guild["id"].as_str(), &guild["voice_states"]);
                }
            }
            "GUILD_CREATE" => self.snapshot(data["id"].as_str(), &data["voice_states"]),
            "GUILD_DELETE" => if let Some(id) = data["id"].as_str() { self.states.retain(|(guild, _), _| guild != id); },
            "VOICE_STATE_UPDATE" => self.update(data, data["guild_id"].as_str(), None),
            "CALL_CREATE" | "CALL_UPDATE" => {
                let channel = data["channel_id"].as_str();
                for state in data["voice_states"].as_array().into_iter().flatten().take(64) { self.update(state, None, channel); }
            }
            "CALL_DELETE" => if let Some(channel) = data["channel_id"].as_str() { self.states.retain(|(guild, _), s| !(guild.is_empty() && s.channel_id == channel)); },
            _ => {}
        }
    }
    /// A full guild snapshot replaces whatever was known for that guild.
    fn snapshot(&mut self, guild: Option<&str>, states: &Value) {
        let Some(guild) = guild else { return };
        let Some(states) = states.as_array() else { return };
        self.states.retain(|(g, _), _| g != guild);
        for state in states.iter().take(5000) { self.update(state, Some(guild), None); }
    }
    fn update(&mut self, data: &Value, guild: Option<&str>, channel: Option<&str>) {
        let Some(user) = data["user_id"].as_str().or_else(|| data["member"]["user"]["id"].as_str()).filter(|u| !u.is_empty()) else { return };
        let key = (guild.unwrap_or("").to_owned(), user.to_owned());
        let Some(channel) = data["channel_id"].as_str().or(channel).filter(|c| !c.is_empty()) else {
            self.states.remove(&key);
            return;
        };
        if self.states.len() >= LIMIT && !self.states.contains_key(&key) { return; }
        let flag = |name: &str| data[name].as_bool() == Some(true);
        let member_user = serde_json::from_value::<User>(data["member"]["user"].clone()).ok().filter(|u| !u.id.is_empty());
        let previous = self.states.get(&key).filter(|s| s.channel_id == channel);
        let state = VoiceState {
            user_id: user.to_owned(),
            guild_id: guild.map(str::to_owned),
            channel_id: channel.to_owned(),
            muted: flag("mute") || flag("self_mute") || flag("suppress"),
            deafened: flag("deaf") || flag("self_deaf"),
            video: flag("self_video"),
            streaming: flag("self_stream"),
            user: member_user.or_else(|| previous.and_then(|s| s.user.clone())),
            nick: data["member"]["nick"].as_str().map(str::to_owned).or_else(|| previous.and_then(|s| s.nick.clone())),
        };
        self.states.insert(key, state);
    }
    /// Everyone currently in a voice channel.
    pub fn in_channel(&self, channel: &str) -> Vec<VoiceState> {
        let mut states: Vec<_> = self.states.values().filter(|s| s.channel_id == channel).cloned().collect();
        states.sort_by(|a, b| a.user_id.cmp(&b.user_id));
        states
    }
    pub fn clear(&mut self) { self.states.clear(); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn snapshots_updates_moves_and_leaves() {
        let mut roster = VoiceRoster::default();
        roster.ingest("READY", &json!({"guilds":[{"id":"1","voice_states":[{"user_id":"10","channel_id":"100","self_mute":true},{"user_id":"11","channel_id":"100","self_stream":true}]}]}));
        let states = roster.in_channel("100");
        assert_eq!(states.len(), 2);
        assert!(states[0].muted && !states[0].streaming && states[1].streaming);
        // Another client joins, with member data attached.
        roster.ingest("VOICE_STATE_UPDATE", &json!({"guild_id":"1","user_id":"12","channel_id":"101","self_deaf":true,"member":{"nick":"Nick","user":{"id":"12","username":"twelve"}}}));
        let joined = &roster.in_channel("101")[0];
        assert!(joined.deafened && joined.nick.as_deref() == Some("Nick") && joined.user.as_ref().unwrap().username == "twelve");
        // Moving channels and leaving.
        roster.ingest("VOICE_STATE_UPDATE", &json!({"guild_id":"1","user_id":"10","channel_id":"101"}));
        assert_eq!(roster.in_channel("100").len(), 1);
        assert_eq!(roster.in_channel("101").len(), 2);
        roster.ingest("VOICE_STATE_UPDATE", &json!({"guild_id":"1","user_id":"10","channel_id":null}));
        assert_eq!(roster.in_channel("101").len(), 1);
        // A fresh guild snapshot replaces stale state; DM calls end with CALL_DELETE.
        roster.ingest("GUILD_CREATE", &json!({"id":"1","voice_states":[]}));
        assert!(roster.in_channel("100").is_empty() && roster.in_channel("101").is_empty());
        roster.ingest("CALL_CREATE", &json!({"channel_id":"55","voice_states":[{"user_id":"10","channel_id":"55"}]}));
        assert_eq!(roster.in_channel("55").len(), 1);
        roster.ingest("CALL_DELETE", &json!({"channel_id":"55"}));
        assert!(roster.in_channel("55").is_empty());
    }
}
