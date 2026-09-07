//! Light mode's conversation store: what the chat window shows, kept between runs.
//!
//! In memory it is a list of chats, each with its messages. On disk it is one JSON
//! file in the data dir, rewritten whole (it is small: text only, capped per chat).
//! serde_json comes for free through whatsapp-rust's re-export, so this adds no
//! dependency; the shapes are built by hand rather than derived, for the same reason.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use whatsapp_rust::serde_json::{self, json, Value};

pub const FILE: &str = "light-chats.json";
/// Per chat. Enough to scroll back through a conversation, small enough to keep whole.
const MAX_PER_CHAT: usize = 500;
/// Coalesce bursts (a history sync is hundreds of messages) into one write.
const SAVE_EVERY: Duration = Duration::from_secs(2);

#[derive(Clone, Debug)]
pub struct Msg {
    pub id: String,
    pub from_me: bool,
    /// Display name of the sender; empty for our own messages.
    pub sender: String,
    /// Unix seconds.
    pub ts: u64,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct Chat {
    pub jid: String,
    pub name: String,
    pub is_group: bool,
    pub last_ts: u64,
    pub unread: u32,
    pub messages: Vec<Msg>,
}

impl Chat {
    pub fn title(&self) -> String {
        if self.name.trim().is_empty() {
            friendly_jid(&self.jid)
        } else {
            self.name.clone()
        }
    }
}

/// A JID with the server part turned into something a person can read.
pub fn friendly_jid(jid: &str) -> String {
    let (user, server) = jid.split_once('@').unwrap_or((jid, ""));
    let user = user.split(':').next().unwrap_or(user);
    match server {
        "s.whatsapp.net" | "c.us" => format!("+{user}"),
        "g.us" => format!("Group {user}"),
        _ => user.to_string(),
    }
}

pub struct Chats {
    chats: Vec<Chat>,
    /// Contact names by JID, from the phone's address book sync. Wins over push names.
    names: HashMap<String, String>,
    path: PathBuf,
    dirty: bool,
    last_save: Instant,
}

impl Chats {
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join(FILE);
        let mut this = Chats {
            chats: Vec::new(),
            names: HashMap::new(),
            path,
            dirty: false,
            last_save: Instant::now(),
        };
        if let Ok(text) = std::fs::read_to_string(&this.path) {
            if let Ok(v) = serde_json::from_str::<Value>(&text) {
                this.from_json(&v);
            }
        }
        this
    }

    /// An in-memory store that never touches disk. For the demo window.
    pub fn ephemeral() -> Self {
        Chats {
            chats: Vec::new(),
            names: HashMap::new(),
            path: PathBuf::new(),
            dirty: false,
            last_save: Instant::now(),
        }
    }

    /// Chats, most recent activity first.
    pub fn sorted(&self) -> Vec<&Chat> {
        let mut v: Vec<&Chat> = self.chats.iter().collect();
        v.sort_by(|a, b| b.last_ts.cmp(&a.last_ts));
        v
    }

    pub fn get(&self, jid: &str) -> Option<&Chat> {
        self.chats.iter().find(|c| c.jid == jid)
    }

    pub fn display_name(&self, jid: &str) -> String {
        self.names
            .get(jid)
            .cloned()
            .or_else(|| self.get(jid).map(|c| c.title()))
            .unwrap_or_else(|| friendly_jid(jid))
    }

    fn chat_mut(&mut self, jid: &str, is_group: bool) -> &mut Chat {
        if let Some(i) = self.chats.iter().position(|c| c.jid == jid) {
            return &mut self.chats[i];
        }
        let name = self.names.get(jid).cloned().unwrap_or_default();
        self.chats.push(Chat {
            jid: jid.to_string(),
            name,
            is_group,
            last_ts: 0,
            unread: 0,
            messages: Vec::new(),
        });
        self.dirty = true;
        self.chats.last_mut().unwrap()
    }

    /// Record a message. Returns false if it was already known (same id).
    pub fn push(&mut self, jid: &str, is_group: bool, msg: Msg, count_unread: bool) -> bool {
        let chat = self.chat_mut(jid, is_group);
        if !msg.id.is_empty() && chat.messages.iter().any(|m| m.id == msg.id) {
            return false;
        }
        // A direct chat with no name yet takes the peer's push name.
        if !is_group && !msg.from_me && chat.name.is_empty() && !msg.sender.is_empty() {
            chat.name = msg.sender.clone();
        }
        chat.last_ts = chat.last_ts.max(msg.ts);
        if count_unread && !msg.from_me {
            chat.unread += 1;
        }
        // Keep chronological order; history arrives oldest-first, live messages append.
        let at = chat
            .messages
            .iter()
            .rposition(|m| m.ts <= msg.ts)
            .map(|i| i + 1)
            .unwrap_or(0);
        chat.messages.insert(at, msg);
        if chat.messages.len() > MAX_PER_CHAT {
            let excess = chat.messages.len() - MAX_PER_CHAT;
            chat.messages.drain(..excess);
        }
        self.dirty = true;
        true
    }

    /// Name a chat (from history sync or a group subject). Empty names are ignored.
    pub fn name_chat(&mut self, jid: &str, is_group: bool, name: &str) {
        if name.trim().is_empty() {
            return;
        }
        let chat = self.chat_mut(jid, is_group);
        if chat.name != name {
            chat.name = name.to_string();
            self.dirty = true;
        }
    }

    /// An address-book name for a JID. Applies to the chat too, if there is one.
    pub fn set_contact_name(&mut self, jid: &str, name: &str) {
        if name.trim().is_empty() {
            return;
        }
        self.names.insert(jid.to_string(), name.to_string());
        if let Some(c) = self.chats.iter_mut().find(|c| c.jid == jid) {
            c.name = name.to_string();
        }
        self.dirty = true;
    }

    pub fn mark_read(&mut self, jid: &str) {
        if let Some(c) = self.chats.iter_mut().find(|c| c.jid == jid) {
            if c.unread != 0 {
                c.unread = 0;
                self.dirty = true;
            }
        }
    }

    /// Write to disk if anything changed and the debounce has elapsed (or `force`).
    pub fn save_if_due(&mut self, force: bool) {
        if self.path.as_os_str().is_empty()
            || !self.dirty
            || (!force && self.last_save.elapsed() < SAVE_EVERY)
        {
            return;
        }
        let text = self.to_json().to_string();
        let tmp = self.path.with_extension("json.tmp");
        if std::fs::write(&tmp, text).is_ok() && std::fs::rename(&tmp, &self.path).is_ok() {
            self.dirty = false;
            self.last_save = Instant::now();
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "names": self.names,
            "chats": self.chats.iter().map(|c| json!({
                "jid": c.jid,
                "name": c.name,
                "is_group": c.is_group,
                "last_ts": c.last_ts,
                "unread": c.unread,
                "messages": c.messages.iter().map(|m| json!({
                    "id": m.id, "from_me": m.from_me, "sender": m.sender, "ts": m.ts, "text": m.text,
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })
    }

    fn from_json(&mut self, v: &Value) {
        let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let u = |v: &Value, k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
        let b = |v: &Value, k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
        if let Some(names) = v.get("names").and_then(Value::as_object) {
            for (k, n) in names {
                if let Some(n) = n.as_str() {
                    self.names.insert(k.clone(), n.to_string());
                }
            }
        }
        for c in v.get("chats").and_then(Value::as_array).into_iter().flatten() {
            let messages = c
                .get("messages")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|m| Msg {
                    id: s(m, "id"),
                    from_me: b(m, "from_me"),
                    sender: s(m, "sender"),
                    ts: u(m, "ts"),
                    text: s(m, "text"),
                })
                .collect();
            self.chats.push(Chat {
                jid: s(c, "jid"),
                name: s(c, "name"),
                is_group: b(c, "is_group"),
                last_ts: u(c, "last_ts"),
                unread: u(c, "unread") as u32,
                messages,
            });
        }
    }
}
