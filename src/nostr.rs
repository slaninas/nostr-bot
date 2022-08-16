use secp256k1::Secp256k1;

use log::debug;
use serde::{Deserialize, Serialize};
use std::fmt::Write;

#[derive(Serialize, Deserialize)]
pub struct Message {
    pub msg_type: String,
    pub subscription_id: String,
    pub content: Event,
}

/// Holds non signed nostr event, for more info about events see
/// <https://github.com/nostr-protocol/nips/blob/master/01.md#events-and-signatures>.
pub struct EventNonSigned {
    pub created_at: u64,
    pub kind: u64,
    pub tags: Vec<Vec<String>>,
    pub content: String,
}

impl EventNonSigned {
    /// Returns event signed by `keypair`.
    pub fn sign(self, keypair: &secp256k1::KeyPair) -> Event {
        Event::new(keypair, self.created_at, self.kind, self.tags, self.content)
    }
}

/// Holds nostr event, see
/// <https://github.com/nostr-protocol/nips/blob/master/01.md#events-and-signatures>.
#[derive(Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub pubkey: String,
    pub created_at: u64,
    pub kind: u64,
    pub tags: Vec<Vec<String>>,
    pub content: String,
    pub sig: String,
}

impl Event {
    /// Creates a new nostr event and signs it using `keypair`.
    pub fn new(
        keypair: &secp256k1::KeyPair,
        created_at: u64,
        kind: u64,
        tags: Vec<Vec<String>>,
        content: String,
    ) -> Self {
        let content = escape(content);

        let secp = Secp256k1::new();

        let (pubkey, _parity) = keypair.x_only_public_key();

        let mut formatted_tags = Self::format_tags(&tags);
        formatted_tags.retain(|c| !c.is_whitespace());

        let msg = format!(
            r#"[0,"{}",{},{},[{}],"{}"]"#,
            pubkey, created_at, kind, formatted_tags, content
        );
        debug!("commitment '{}'\n", msg);
        let id =
            secp256k1::Message::from_hashed_data::<secp256k1::hashes::sha256::Hash>(msg.as_bytes());

        let signature = secp.sign_schnorr(&id, keypair);

        Event {
            id: id.to_string(),
            pubkey: pubkey.to_string(),
            created_at,
            kind,
            content,
            sig: signature.to_string(),
            tags,
        }
    }

    /// Formats an event into string which can send to relays.
    pub fn format(&self) -> String {
        format!(
            r#"["EVENT",{{"id": "{}", "pubkey": "{}", "created_at": {}, "kind": {}, "tags": [{}], "content": "{}", "sig": "{}"}}]"#,
            self.id,
            self.pubkey,
            self.created_at,
            self.kind,
            Self::format_tags(&self.tags),
            &self.content,
            self.sig
        )
    }

    fn format_tags(tags: &Vec<Vec<String>>) -> String {
        let mut formatted = String::new();

        for i in 0..tags.len() {
            let tag = &tags[i];
            write!(formatted, r#"["{}"]"#, tag.join(r#"", ""#)).unwrap();
            if i + 1 < tags.len() {
                formatted.push_str(", ");
            }
        }
        formatted
    }
}

/// Returns tags that can be used to form event that is a reply to `event`.
///
/// This takes first "e" tag from the `event` (=root of the thread) and `event`s `id` and alo adds
/// pubkey of the `event` author to the tags.
pub fn tags_for_reply(event: Event) -> Vec<Vec<String>> {
    let mut e_tags = vec![];
    let mut p_tags = vec![];

    for t in event.tags {
        if t[0] == "e" {
            e_tags.push(t);
        } else if t[0] == "p" {
            p_tags.push(t);
        }
    }

    // Mention only author of the event
    let mut tags = vec![vec!["p".to_string(), event.pubkey]];

    // First event and event I'm going to reply to
    if !e_tags.is_empty() {
        tags.push(e_tags[0].clone());
    }
    tags.push(vec!["e".to_string(), event.id]);

    debug!("Returning these tags: {:?}", tags);
    tags
}

pub fn get_profile_event(
    name: Option<String>,
    about: Option<String>,
    picture_url: Option<String>,
) -> EventNonSigned {
    let name = if let Some(name) = name {
        name
    } else {
        "".to_string()
    };
    let about = if let Some(about) = about {
        about
    } else {
        "".to_string()
    };
    let picture_url = if let Some(picture_url) = picture_url {
        picture_url
    } else {
        "".to_string()
    };

    EventNonSigned {
        created_at: crate::utils::unix_timestamp(),
        kind: 0,
        tags: vec![],
        content: format!(
            r#"{{"name":"{}","about":"{}","picture":"{}"}}"#,
            name,
            escape(about),
            escape(picture_url)
        ),
    }
}

/// Returns [EventNonSigned] that is a reply to given `reply_to` event with content set to `content`.
pub fn get_reply(reply_to: Event, content: String) -> EventNonSigned {
    EventNonSigned {
        content,
        created_at: crate::utils::unix_timestamp(),
        kind: 1,
        tags: tags_for_reply(reply_to),
    }
}

fn escape(text: String) -> String {
    // See https://github.com/jb55/nostril/blob/master/nostril.c.
    let mut escaped = String::new();
    for c in text.chars() {
        match c {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(c),
        }
    }

    escaped
}
