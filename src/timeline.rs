use crate::model::Message;
use std::collections::VecDeque;
#[derive(Clone)] pub struct Logged { pub message: Message, pub deleted: bool }
/// Replies start a new group so their context stays beside their author.
pub fn grouped(previous: &Message, current: &Message) -> bool {
    !current.author.id.is_empty()
        && previous.channel_id == current.channel_id
        && previous.author.id == current.author.id
        && current.referenced_message.is_none()
}
// Deleted snapshots rejoin the current channel's chronology, without duplicating live IDs.
pub fn messages(live: &VecDeque<Message>, logs: &VecDeque<Logged>, channel: &str) -> Vec<Message> {
    let mut result: Vec<_>=live.iter().filter(|m|m.channel_id==channel).cloned().collect();
    for entry in logs.iter().rev().filter(|e|e.deleted&&e.message.channel_id==channel) {
        if !result.iter().any(|m|m.id==entry.message.id) {result.push(entry.message.clone());}
    }
    result.sort_by(|a,b| a.timestamp.cmp(&b.timestamp).then_with(||a.id.parse::<u64>().unwrap_or(0).cmp(&b.id.parse::<u64>().unwrap_or(0))));
    result
}
pub fn is_edit(message:&Message,update:&serde_json::Value)->bool {
    update["content"].as_str().is_some_and(|content|content!=message.content)
        ||update["edited_timestamp"].as_str().is_some_and(|timestamp|Some(timestamp)!=message.edited_timestamp.as_deref())
}
#[cfg(test)] mod tests {
    use super::*;
    fn message(id:&str,channel:&str,time:&str)->Message {Message{id:id.into(),channel_id:channel.into(),timestamp:time.into(),..Default::default()}}
    #[test] fn groups_same_author_but_keeps_replies_and_channels_separate() {
        let mut first=message("1","a","01");first.author.id="user".into();
        let mut second=first.clone();second.id="2".into();
        assert!(grouped(&first,&second));
        second.author.id="another".into();assert!(!grouped(&first,&second));
        second.author.id=first.author.id.clone();second.channel_id="b".into();assert!(!grouped(&first,&second));
        second.channel_id=first.channel_id.clone();second.referenced_message=Some(Box::new(first.clone()));assert!(!grouped(&first,&second));
        second.referenced_message=None;first.author.id.clear();second.author.id.clear();assert!(!grouped(&first,&second));
    }
    #[test] fn delete_snapshots_survive_channel_reload_without_duplicates_or_cross_channel_leaks() {
        let live=VecDeque::from([message("20","a","02"),message("30","a","03")]);
        let logs=VecDeque::from([Logged{message:message("10","a","01"),deleted:true},Logged{message:message("20","a","02"),deleted:false},Logged{message:message("30","a","03"),deleted:true},Logged{message:message("40","b","04"),deleted:true}]);
        assert_eq!(messages(&live,&logs,"a").iter().map(|m|m.id.as_str()).collect::<Vec<_>>(),["10","20","30"]);
    }
    #[test] fn attachment_edits_are_retained_but_embed_refreshes_and_duplicate_updates_are_not() {
        let mut message=message("1","a","01");message.content="hello".into();
        assert!(is_edit(&message,&serde_json::json!({"edited_timestamp":"02","attachments":[]})));
        assert!(!is_edit(&message,&serde_json::json!({"embeds":[]})));
        message.edited_timestamp=Some("02".into());assert!(!is_edit(&message,&serde_json::json!({"edited_timestamp":"02","content":"hello"})));
        assert!(is_edit(&message,&serde_json::json!({"content":"changed"})));
    }
}
