use serde::{Deserialize, Serialize};
use serde_json::Value;

const API: &str = "https://discord.com/api/v10";

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct User { pub id: String, pub username: String, pub global_name: Option<String>, pub avatar: Option<String> }
pub fn user_avatar_url(user: &User) -> Option<String> {
    user.avatar.as_ref().map(|h| format!("https://cdn.discordapp.com/avatars/{}/{}.png", user.id, h))
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Guild { pub id: String, pub name: String, pub icon: Option<String> }
pub fn guild_icon_url(guild: &Guild) -> Option<String> {
    guild.icon.as_ref().map(|h| format!("https://cdn.discordapp.com/icons/{}/{}.png", guild.id, h))
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Channel { pub id: String, pub name: Option<String>, #[serde(rename="type")] pub kind: i64, #[serde(default)] pub recipients: Vec<User> }

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Attachment { pub id: String, pub url: String, pub filename: String }

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Message { pub id: String, pub content: String, pub author: User, #[serde(default)] pub attachments: Vec<Attachment> }

pub fn me(token: &str) -> Result<User, String> {
    let resp = reqwest::blocking::Client::new()
        .get(format!("{}/users/@me", API))
        .header("Authorization", token)
        .send().map_err(|e| e.to_string())?;
    resp.json().map_err(|e| e.to_string())
}

pub fn guilds(token: &str) -> Result<Vec<Guild>, String> {
    let resp = reqwest::blocking::Client::new()
        .get(format!("{}/users/@me/guilds", API))
        .header("Authorization", token)
        .send().map_err(|e| e.to_string())?;
    resp.json().map_err(|e| e.to_string())
}

pub fn channels(token: &str, guild_id: &str) -> Result<Vec<Channel>, String> {
    let resp = reqwest::blocking::Client::new()
        .get(format!("{}/guilds/{}/channels", API, guild_id))
        .header("Authorization", token)
        .send().map_err(|e| e.to_string())?;
    resp.json().map_err(|e| e.to_string())
}

pub fn messages(token: &str, channel_id: &str, limit: u32) -> Result<Vec<Message>, String> {
    let resp = reqwest::blocking::Client::new()
        .get(format!("{}/channels/{}/messages?limit={}", API, channel_id, limit))
        .header("Authorization", token)
        .send().map_err(|e| e.to_string())?;
    resp.json().map_err(|e| e.to_string())
}

pub fn send_message(token: &str, channel_id: &str, content: &str) -> Result<Value, String> {
    let body = serde_json::json!({"content": content});
    let resp = reqwest::blocking::Client::new()
        .post(format!("{}/channels/{}/messages", API, channel_id))
        .header("Authorization", token)
        .json(&body)
        .send().map_err(|e| e.to_string())?;
    resp.json().map_err(|e| e.to_string())
}


pub fn dm_channels(token: &str) -> Result<Vec<Channel>, String> {
    let resp = reqwest::blocking::Client::new()
        .get(format!("{}/users/@me/channels", API))
        .header("Authorization", token)
        .send().map_err(|e| e.to_string())?;
    resp.json().map_err(|e| e.to_string())
}

pub fn close_dm(token: &str, channel_id: &str) -> Result<reqwest::blocking::Response, String> {
    let resp = reqwest::blocking::Client::new()
        .delete(format!("{}/users/@me/channels/{}", API, channel_id))
        .header("Authorization", token)
        .send().map_err(|e| e.to_string())?;
    Ok(resp)
}

pub fn block_user(token: &str, user_id: &str) -> Result<reqwest::blocking::Response, String> {
    let resp = reqwest::blocking::Client::new()
        .put(format!("{}/users/@me/relationships/{}", API, user_id))
        .header("Authorization", token)
        .json(&serde_json::json!({"type": 2}))
        .send().map_err(|e| e.to_string())?;
    Ok(resp)
}

pub fn unfriend_user(token: &str, user_id: &str) -> Result<reqwest::blocking::Response, String> {
    let resp = reqwest::blocking::Client::new()
        .delete(format!("{}/users/@me/relationships/{}", API, user_id))
        .header("Authorization", token)
        .send().map_err(|e| e.to_string())?;
    Ok(resp)
}

pub fn leave_guild(token: &str, guild_id: &str) -> Result<reqwest::blocking::Response, String> {
    let resp = reqwest::blocking::Client::new()
        .delete(format!("{}/users/@me/guilds/{}", API, guild_id))
        .header("Authorization", token)
        .send().map_err(|e| e.to_string())?;
    Ok(resp)
}

pub fn create_invite(token: &str, guild_id: &str) -> Result<serde_json::Value, String> {
    let body = serde_json::json!({
        "max_age": 86400,
        "max_uses": 0,
        "temporary": false,
        "unique": true
    });
    let resp = reqwest::blocking::Client::new()
        .post(format!("{}/guilds/{}/invites", API, guild_id))
        .header("Authorization", token)
        .json(&body)
        .send().map_err(|e| e.to_string())?;
    resp.json().map_err(|e| e.to_string())
}

pub fn mute_guild(token: &str, guild_id: &str) -> Result<reqwest::blocking::Response, String> {
    let body = serde_json::json!({"message_notifications": 0});
    let resp = reqwest::blocking::Client::new()
        .patch(format!("{}/users/@me/guilds/{}", API, guild_id))
        .header("Authorization", token)
        .json(&body)
        .send().map_err(|e| e.to_string())?;
    Ok(resp)
}


