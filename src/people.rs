use crate::{community::Relationship,model::User};
use std::collections::{HashMap,HashSet};
pub fn search(query:&str, me:&str, relationships:&[Relationship], known:impl Iterator<Item=(User,Option<String>)>)->Vec<(User,Option<String>)> {
    let blocked:HashSet<_>=relationships.iter().filter(|r|r.kind==2).map(|r|r.id.as_str()).collect();
    let mut people=HashMap::new();
    for (user,nick) in known.chain(relationships.iter().filter(|r|r.kind!=2).map(|r|(r.user.clone(),r.nickname.clone()))) {
        if user.id!=me&&!user.username.is_empty()&&!blocked.contains(user.id.as_str()){people.entry(user.id.clone()).and_modify(|entry:&mut(User,Option<String>)|{if entry.1.is_none(){entry.1=nick.clone();}}).or_insert((user,nick));}
    }
    let query=query.trim().trim_start_matches('@').to_lowercase();
    let mut result:Vec<_>=people.into_values().filter(|(u,n)|[Some(u.name()),Some(u.username.as_str()),n.as_deref()].into_iter().flatten().any(|s|s.to_lowercase().contains(&query))).collect();
    result.sort_by_key(|(u,_)|(!u.name().to_lowercase().starts_with(&query),u.name().to_lowercase(),u.id.clone()));result.truncate(80);result
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn name_search_matches_nicknames_deduplicates_and_excludes_self_and_blocks() {
        let user=|id:&str,name:&str|User{id:id.into(),username:name.into(),..Default::default()};
        let rel=vec![Relationship{id:"3".into(),user:user("3","Alex Blocked"),kind:2,..Default::default()}];
        let known=vec![(user("1","Alex Me"),None),(user("2","someone"),None),(user("2","someone"),Some("Alex".into())),(user("3","Alex Blocked"),None)];
        let found=search("alex","1",&rel,known.into_iter());assert_eq!(found.len(),1);assert_eq!(found[0].0.id,"2");
        assert!(search("unknown","1",&rel,std::iter::empty()).is_empty());
    }
}
