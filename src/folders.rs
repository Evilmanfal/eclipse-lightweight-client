//! Read-only subset of Discord's preloaded settings; unknown fields are ignored.
//! Field numbers: discord-userdoccers/discord-protos, PreloadedUserSettings.proto.
use crate::model::Guild;
use base64::{engine::general_purpose::STANDARD, Engine};
use prost::Message;
use serde_json::Value;
use std::collections::HashSet;

#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub folders: Vec<Folder>,
    pub positions: Vec<String>,
}
#[derive(Clone, Debug, Default)]
pub struct Folder {
    pub id: Option<String>,
    pub name: Option<String>,
    pub color: Option<u32>,
    pub guild_ids: Vec<String>,
}
#[derive(Clone, Debug)]
pub enum RailEntry {
    Server(Guild),
    Folder {
        key: String,
        name: String,
        color: Option<u32>,
        guilds: Vec<Guild>,
    },
}

#[derive(prost::Message)]
struct Settings {
    #[prost(message, optional, tag = "14")]
    guild_folders: Option<GuildFolders>,
}
#[derive(prost::Message)]
struct GuildFolders {
    #[prost(message, repeated, tag = "1")]
    folders: Vec<GuildFolder>,
    #[prost(fixed64, repeated, tag = "2")]
    guild_positions: Vec<u64>,
}
#[derive(prost::Message)]
struct GuildFolder {
    #[prost(fixed64, repeated, tag = "1")]
    guild_ids: Vec<u64>,
    #[prost(message, optional, tag = "2")]
    id: Option<Int64Value>,
    #[prost(message, optional, tag = "3")]
    name: Option<StringValue>,
    #[prost(message, optional, tag = "4")]
    color: Option<UInt64Value>,
}
#[derive(prost::Message)]
struct Int64Value {
    #[prost(int64, tag = "1")]
    value: i64,
}
#[derive(prost::Message)]
struct UInt64Value {
    #[prost(uint64, tag = "1")]
    value: u64,
}
#[derive(prost::Message)]
struct StringValue {
    #[prost(string, tag = "1")]
    value: String,
}

pub fn decode(encoded: &str) -> Result<Option<Layout>, String> {
    if encoded.len() > 6 * 1024 * 1024 {
        return Err("Server folder settings are too large.".into());
    }
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| "Invalid server folder encoding.")?;
    let settings =
        Settings::decode(bytes.as_slice()).map_err(|_| "Could not decode server folders.")?;
    // An unrelated partial settings event must not erase the current folders.
    Ok(settings.guild_folders.map(|f| Layout {
        folders: f
            .folders
            .into_iter()
            .take(1000)
            .map(|f| Folder {
                id: f.id.map(|v| v.value.to_string()),
                name: f.name.map(|v| v.value.chars().take(100).collect()),
                color: f.color.map(|v| (v.value & 0xFFFFFF) as u32),
                guild_ids: f
                    .guild_ids
                    .into_iter()
                    .take(1000)
                    .map(|id| id.to_string())
                    .collect(),
            })
            .collect(),
        positions: f
            .guild_positions
            .into_iter()
            .take(1000)
            .map(|id| id.to_string())
            .collect(),
    }))
}

fn id(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_u64().map(|v| v.to_string()))
}
pub fn legacy(value: &Value) -> Option<Layout> {
    let entries = value.get("guild_folders")?.as_array()?;
    Some(Layout {
        folders: entries
            .iter()
            .take(1000)
            .map(|f| Folder {
                id: id(&f["id"]),
                name: f["name"].as_str().map(|s| s.chars().take(100).collect()),
                color: f["color"].as_u64().map(|n| (n & 0xFFFFFF) as u32),
                guild_ids: f["guild_ids"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(1000)
                    .filter_map(id)
                    .collect(),
            })
            .collect(),
        positions: value["guild_positions"]
            .as_array()
            .into_iter()
            .flatten()
            .take(1000)
            .filter_map(id)
            .collect(),
    })
}

impl Layout {
    pub fn rail(&self, guilds: &[Guild]) -> Vec<RailEntry> {
        let available: std::collections::HashMap<_, _> =
            guilds.iter().map(|g| (g.id.as_str(), g)).collect();
        let mut seen = HashSet::new();
        let mut result = Vec::new();
        for folder in &self.folders {
            let servers: Vec<_> = folder
                .guild_ids
                .iter()
                .filter_map(|id| {
                    available
                        .get(id.as_str())
                        .filter(|_| seen.insert(id.clone()))
                        .map(|g| (*g).clone())
                })
                .collect();
            if servers.is_empty() {
                continue;
            }
            if let Some(id) = &folder.id {
                result.push(RailEntry::Folder {
                    key: id.clone(),
                    name: folder
                        .name
                        .clone()
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| {
                            servers
                                .iter()
                                .map(|g| g.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        }),
                    color: folder.color,
                    guilds: servers,
                });
            } else {
                result.extend(servers.into_iter().map(RailEntry::Server));
            }
        }
        // Keep servers visible if settings contain stale or missing entries.
        for id in self
            .positions
            .iter()
            .map(String::as_str)
            .chain(guilds.iter().map(|g| g.id.as_str()))
        {
            if let Some(guild) = available.get(id).filter(|_| seen.insert(id.to_owned())) {
                result.push(RailEntry::Server((*guild).clone()));
            }
        }
        result
    }
    pub fn demo() -> Self {
        Self {
            folders: vec![
                Folder {
                    guild_ids: vec!["local-lab".into()],
                    ..Default::default()
                },
                Folder {
                    id: Some("creative".into()),
                    name: Some("Creative spaces".into()),
                    color: Some(0x41B7CD),
                    guild_ids: vec!["local-design".into(), "local-night".into()],
                },
                Folder {
                    guild_ids: vec!["local-build".into()],
                    ..Default::default()
                },
            ],
            positions: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decodes_real_wire_types_wrappers_and_large_snowflakes() {
        // Hand-written wire fixture, not encoded with the structs being tested.
        // field14 -> folders[ ids packed fixed64, id Int64Value, name StringValue, color UInt64Value ]; positions.
        let bytes = [
            0x72, 0x25, 0x0A, 0x19, 0x0A, 0x08, 0x01, 0, 0, 0, 0, 0, 0, 0x10, 0x12, 0x02, 0x08,
            0x07, 0x1A, 0x05, 0x0A, 0x03, b'A', b'r', b't', 0x22, 0x02, 0x08, 0x2A, 0x12, 0x08,
            0x01, 0, 0, 0, 0, 0, 0, 0x10,
        ];
        let layout = decode(&STANDARD.encode(bytes)).unwrap().unwrap();
        assert_eq!(layout.folders[0].id.as_deref(), Some("7"));
        assert_eq!(layout.folders[0].name.as_deref(), Some("Art"));
        assert_eq!(layout.folders[0].color, Some(42));
        assert_eq!(layout.folders[0].guild_ids, vec!["1152921504606846977"]);
        assert_eq!(layout.positions, layout.folders[0].guild_ids);
        assert!(decode(&STANDARD.encode([0x0A, 0x02, 0x08, 0x01]))
            .unwrap()
            .is_none());
        assert!(decode("invalid!").is_err());
        assert!(decode(&STANDARD.encode([0x72, 0x7F])).is_err());
    }
    #[test]
    fn keeps_folder_order_deduplicates_and_preserves_new_servers() {
        let value = serde_json::json!({"guild_folders":[{"id":null,"guild_ids":["3"]},{"id":0,"name":"Art","color":42,"guild_ids":["2","2","gone"]}],"guild_positions":["4","1","2"]});
        let layout = legacy(&value).unwrap();
        let guilds: Vec<_> = ["1", "2", "3", "4"]
            .iter()
            .map(|id| Guild {
                id: id.to_string(),
                name: id.to_string(),
                ..Default::default()
            })
            .collect();
        let rail = layout.rail(&guilds);
        assert!(matches!(&rail[0],RailEntry::Server(g) if g.id=="3"));
        assert!(
            matches!(&rail[1],RailEntry::Folder{key,guilds,..} if key=="0"&&guilds.len()==1&&guilds[0].id=="2")
        );
        assert!(matches!(&rail[2],RailEntry::Server(g) if g.id=="4"));
        assert!(matches!(&rail[3],RailEntry::Server(g) if g.id=="1"));
    }
}
