use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const HISTORY_LIMIT: usize = 200;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct User {
    pub id: String,
    #[serde(default)]
    pub username: String,
    pub global_name: Option<String>,
    pub avatar: Option<String>,
    #[serde(default)]
    pub discriminator: String,
    #[serde(default)]
    pub premium_type: u8,
    pub bio: Option<String>,
    pub pronouns: Option<String>,
    pub banner: Option<String>,
    pub accent_color: Option<u32>,
    pub avatar_decoration_data: Option<serde_json::Value>,
    pub collectibles: Option<serde_json::Value>,
    pub display_name_styles: Option<serde_json::Value>,
    pub primary_guild: Option<serde_json::Value>,
    #[serde(default)] pub public_flags: u64,
}
impl User {
    pub fn name(&self) -> &str {
        self.global_name
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.username)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Guild {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    #[serde(default)]
    pub owner: bool,
    pub owner_id: Option<String>,
    pub permissions: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Channel {
    pub id: String,
    pub last_message_id:Option<String>,
    #[serde(default)] pub rate_limit_per_user:u32,
    pub bitrate:Option<u32>,
    pub user_limit:Option<u32>,
    #[serde(default)] pub nsfw:bool,
    #[serde(default)]
    pub guild_id: Option<String>,
    pub name: Option<String>,
    pub topic: Option<String>,
    #[serde(rename = "type", default)]
    pub kind: u8,
    #[serde(default)]
    pub position: i32,
    pub parent_id: Option<String>,
    #[serde(default)]
    pub recipients: Vec<User>,
    pub icon: Option<String>,
}
impl Channel {
    pub fn label(&self) -> String {
        self.name.clone().unwrap_or_else(|| {
            let names: Vec<_> = self.recipients.iter().map(User::name).collect();
            if names.is_empty() {
                "Direct message".into()
            } else {
                names.join(", ")
            }
        })
    }
    pub fn is_text(&self) -> bool {
        matches!(self.kind, 0 | 1 | 3 | 5 | 10 | 11 | 12)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Attachment {
    pub id: String,
    pub filename: String,
    pub url: String,
    #[serde(default)]
    pub size: u64,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Emoji {
    pub id: Option<String>,
    pub name: Option<String>,
}
impl Emoji {
    pub fn route(&self) -> String {
        match &self.id {
            Some(id) => format!("{}:{}", self.name.as_deref().unwrap_or("emoji"), id),
            None => self.name.clone().unwrap_or_default(),
        }
    }
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Reaction {
    pub count: u32,
    #[serde(default)]
    pub me: bool,
    pub emoji: Emoji,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Embed {
    #[serde(rename="type",default)] pub kind:String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub url: Option<String>,
    pub image: Option<EmbedImage>,
    pub thumbnail: Option<EmbedImage>,
    pub video: Option<EmbedImage>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct EmbedImage {
    pub url: Option<String>,
    pub proxy_url: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Message {
    pub id: String,
    pub channel_id: String,
    #[serde(default)]
    pub author: User,
    pub member: Option<MemberProfile>,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub timestamp: String,
    pub edited_timestamp: Option<String>,
    pub referenced_message: Option<Box<Message>>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    #[serde(default)]
    pub embeds: Vec<Embed>,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct MemberProfile {
    pub avatar: Option<String>,
    pub nick: Option<String>,
    #[serde(default)]
    pub roles: Vec<String>,
}

pub fn merge_message(messages: &mut VecDeque<Message>, message: Message) {
    if let Some(existing) = messages.iter_mut().find(|m| m.id == message.id) {
        *existing = message;
    } else {
        messages.push_back(message);
    }
    while messages.len() > HISTORY_LIMIT {
        messages.pop_front();
    }
}

pub fn history(messages: Vec<Message>) -> VecDeque<Message> {
    let mut sorted = messages;
    sorted.sort_by(|a, b| a.id.len().cmp(&b.id.len()).then(a.id.cmp(&b.id)));
    let mut result = VecDeque::new();
    for message in sorted {
        merge_message(&mut result, message);
    }
    result
}

pub fn normalize_token(input: &str) -> Result<String, String> {
    let token = input.trim();
    if token.is_empty() {
        return Err("Paste your own account token to connect.".into());
    }
    if token.chars().any(char::is_whitespace) || token.contains('"') || token.len() < 20 {
        return Err("The token contains unexpected characters or is too short.".into());
    }
    Ok(token.to_owned())
}

pub fn safe_link(link: &str) -> bool {
    reqwest::Url::parse(link).is_ok_and(|u| matches!(u.scheme(), "http" | "https"))
}

pub fn demo() -> (User, Vec<Guild>, Vec<Channel>, VecDeque<Message>) {
    let user = User {
        id: "local-me".into(),
        username: "You".into(),
        global_name: None,
        ..Default::default()
    };
    let guilds = [
        ("local-lab", "Eclipse Lab"),
        ("local-design", "Design Circle"),
        ("local-night", "Night Shift"),
        ("local-build", "Build Guild"),
    ]
    .into_iter()
    .map(|(id, name)| Guild {
        id: id.into(),
        name: name.into(),
        ..Default::default()
    })
    .collect();
    let mut channels: Vec<Channel> = [
        "🐾・𝐠𝐞𝐧𝐞𝐫𝐚𝐥",
        "announcements",
        "introductions",
        "📷・𝓼𝓱𝓸𝔀𝓬𝓪𝓼𝓮",
        "help-and-questions",
        "development",
        "theme-support",
        "off-topic",
    ]
    .iter()
    .enumerate()
    .map(|(i, name)| Channel {
        id: format!("local-{i}"),
        guild_id: Some("local-lab".into()),
        name: Some(name.to_string()),
        topic: Some("a little more room to breathe".into()),
        position: i as i32,
        ..Default::default()
    })
    .collect();
    channels.insert(
        0,
        Channel {
            id: "local-community".into(),
            name: Some("Community".into()),
            kind: 4,
            ..Default::default()
        },
    );
    channels.insert(
        5,
        Channel {
            id: "local-workshop".into(),
            name: Some("Workshop".into()),
            kind: 4,
            ..Default::default()
        },
    );
    channels.push(Channel{id:"local-voice".into(),guild_id:Some("local-lab".into()),name:Some("Lounge".into()),kind:2,..Default::default()});
    let mut messages: VecDeque<Message> = VecDeque::new();
    for (i, (name, text, time)) in [
        (
            "Eclipse",
            "Welcome to Eclipse Lab. Make yourself at home.",
            "4:18 PM",
        ),
        (
            "Alex",
            "That charcoal theme is looking really good.",
            "4:19 PM",
        ),
        (
            "Mira",
            "The rounded panels and cyan accents make everything feel a little calmer.",
            "4:19 PM",
        ),
        ("You", "Exactly the feeling I was going for.", "4:20 PM"),
        (
            "Jordan",
            "@You the message highlights are a nice touch.",
            "4:21 PM",
        ),
        (
            "Noah",
            "And it is all drawn natively. Plenty of room left for the conversation.",
            "4:21 PM",
        ),
        (
            "Casey",
            "Try right-clicking a message for reactions, editing, and copying.",
            "4:22 PM",
        ),
        (
            "Alex",
            "The people panel can collapse when you want more space.",
            "4:22 PM",
        ),
        ("Mira", "@You this is the one. Keep the cyan.", "4:23 PM"),
        ("You", "A little more room to breathe.", "4:24 PM"),
    ]
    .into_iter()
    .enumerate()
    {
        let reply = if i == 4 {
            messages.get(3).cloned().map(Box::new)
        } else if i == 8 {
            messages.get(1).cloned().map(Box::new)
        } else {
            None
        };
        messages.push_back(Message {
            id: format!("demo-{i}"),
            channel_id: "local-0".into(),
            author: User {
                id: if name == "You" {
                    "local-me".into()
                } else {
                    name.into()
                },
                username: name.into(),
                global_name: None,
                ..Default::default()
            },
            content: text.into(),
            timestamp: time.into(),
            referenced_message: reply,
            pinned: i == 0,
            reactions: if i == 2 {
                vec![Reaction {
                    count: 3,
                    me: false,
                    emoji: Emoji {
                        id: None,
                        name: Some("✨".into()),
                    },
                }]
            } else {
                vec![]
            },
            ..Default::default()
        });
    }
    (user, guilds, channels, messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deduplicates_gateway_and_rest_messages_and_bounds_history() {
        let mut messages = VecDeque::new();
        for i in 0..250 {
            merge_message(
                &mut messages,
                Message {
                    id: i.to_string(),
                    ..Default::default()
                },
            );
        }
        assert_eq!(messages.len(), HISTORY_LIMIT);
        assert_eq!(messages.front().unwrap().id, "50");
        merge_message(
            &mut messages,
            Message {
                id: "249".into(),
                content: "edited".into(),
                ..Default::default()
            },
        );
        assert_eq!(messages.len(), HISTORY_LIMIT);
        assert_eq!(messages.back().unwrap().content, "edited");
    }
    #[test]
    fn sorts_discord_history_chronologically() {
        let messages = history(
            ["10", "9", "11"]
                .iter()
                .map(|id| Message {
                    id: id.to_string(),
                    ..Default::default()
                })
                .collect(),
        );
        assert_eq!(
            messages.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["9", "10", "11"]
        );
    }
    #[test]
    fn validates_links_without_allowing_local_programs() {
        assert!(safe_link("https://cdn.discordapp.com/a"));
        for link in [
            "file:///C:/secret",
            "javascript:alert(1)",
            "discord://a",
            "C:\\app.exe",
        ] {
            assert!(!safe_link(link));
        }
    }
    #[test]
    fn validates_token_without_changing_it() {
        assert_eq!(
            normalize_token("  abcdefghijklmnopqrstuvwxyz  ").unwrap(),
            "abcdefghijklmnopqrstuvwxyz"
        );
        assert!(normalize_token("Bot abcdefghijklmnopqrstuvwxyz").is_err());
        assert!(normalize_token("\"").is_err());
    }
}
