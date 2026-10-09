use crate::model::Message;
use std::collections::VecDeque;
#[derive(Clone)] pub struct Logged { pub message: Message, pub deleted: bool }
// Deleted snapshots rejoin the current channel's chronology, without duplicating live IDs.
pub fn messages(live: &VecDeque<Message>, logs: &VecDeque<Logged>, channel: &str) -> Vec<Message> {
    let mut result: Vec<_>=live.iter().filter(|m|m.channel_id==channel).cloned().collect();
    for entry in logs.iter().rev().filter(|e|e.deleted&&e.message.channel_id==channel) {
        if !result.iter().any(|m|m.id==entry.message.id) {result.push(entry.message.clone());}
    }
    result.sort_by(|a,b| a.timestamp.cmp(&b.timestamp).then_with(||a.id.parse::<u64>().unwrap_or(0).cmp(&b.id.parse::<u64>().unwrap_or(0))));
    result
}
/// Messages sent by the same person within 7 minutes of the previous one share its avatar and name,
/// like Discord. Replies always start a new group.
pub fn continues(previous:&Message,message:&Message)->bool{
    if previous.author.id.is_empty()||previous.author.id!=message.author.id||message.referenced_message.is_some(){return false;}
    let seconds=crate::message_time::seconds;
    seconds(&previous.timestamp).zip(seconds(&message.timestamp)).is_some_and(|(a,b)|(0..=420).contains(&(b-a)))
}
pub fn is_edit(message:&Message,update:&serde_json::Value)->bool {
    update["content"].as_str().is_some_and(|content|content!=message.content)
        ||update["edited_timestamp"].as_str().is_some_and(|timestamp|Some(timestamp)!=message.edited_timestamp.as_deref())
}
#[cfg(test)] mod tests {
    use super::*;
    fn message(id:&str,channel:&str,time:&str)->Message {Message{id:id.into(),channel_id:channel.into(),timestamp:time.into(),..Default::default()}}
    #[test] fn consecutive_messages_group_until_seven_minutes_pass_or_someone_else_speaks() {
        let by=|author:&str,time:&str|{let mut m=message("1","c",time);m.author.id=author.into();m};
        let first=by("me","2026-10-06T02:56:00.000000+00:00");
        assert!(continues(&first,&by("me","2026-10-06T03:02:59+00:00")));
        assert!(!continues(&first,&by("me","2026-10-06T03:03:01+00:00")));
        assert!(!continues(&first,&by("you","2026-10-06T02:56:30+00:00")));
        let mut reply=by("me","2026-10-06T02:56:30+00:00");reply.referenced_message=Some(Box::new(first.clone()));
        assert!(!continues(&first,&reply));
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
