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

pub struct EventNonSigned {
    pub created_at: u64,
    pub kind: u64,
    pub tags: Vec<Vec<String>>,
    pub content: String,
}

impl EventNonSigned {
    pub fn sign(self, keypair: &secp256k1::KeyPair) -> Event {
        Event::new(keypair, self.created_at, 1, self.tags, self.content)
    }
}

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
    pub fn new(
        keypair: &secp256k1::KeyPair,
        created_at: u64,
        kind: u64,
        tags: Vec<Vec<String>>,
        content: String,
    ) -> Self {
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

    pub fn format(&self) -> String {
        format!(
            r#"["EVENT",{{"id": "{}", "pubkey": "{}", "created_at": {}, "kind": {}, "tags": [{}], "content": "{}", "sig": "{}"}}]"#,
            self.id,
            self.pubkey,
            self.created_at,
            self.kind,
            Self::format_tags(&self.tags),
            self.content,
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

pub fn get_tags_for_reply(event: Event) -> Vec<Vec<String>> {
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

pub fn format_reply(reply_to: Event, content: String) -> EventNonSigned {
    EventNonSigned {
        content: content,
        created_at: crate::utils::unix_timestamp(),
        kind: 1,
        tags: get_tags_for_reply(reply_to),
    }
}
