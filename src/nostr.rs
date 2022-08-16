use secp256k1::Secp256k1;
use std::str::FromStr;

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

        let id = Self::get_id(pubkey, created_at, kind, &tags, &content);
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

    /// Checks whether the signature for the event is valid.
    pub fn has_valid_sig(&self) -> bool {
        let secp = secp256k1::Secp256k1::verification_only();
        // TODO: Hold secp256k1::Message/Signature/XOnlyPublicKey in the Event so it don't have to be recreated
        let pubkey = match secp256k1::XOnlyPublicKey::from_str(&self.pubkey) {
            Ok(pubkey) => pubkey,
            Err(_) => return false,
        };

        let signature = match secp256k1::schnorr::Signature::from_str(&self.sig) {
            Ok(signature) => signature,
            Err(_) => return false,
        };

        let message = Event::get_id(
            pubkey,
            self.created_at,
            self.kind,
            &self.tags,
            &self.content,
        );

        if message.to_string() != self.id {
            return false;
        }

        !secp
            .verify_schnorr(&signature, &message, &pubkey)
            .is_err()
    }

    fn format_tags(tags: &Vec<Vec<String>>) -> String {
        let mut formatted = String::new();

        for i in 0..tags.len() {
            let tag = &tags[i];
            write!(formatted, r#"["{}"]"#, tag.join(r#"",""#)).unwrap();
            if i + 1 < tags.len() {
                formatted.push_str(",");
            }
        }
        println!("formatted tags >{}<", formatted);
        formatted
    }

    fn get_id(
        pubkey: secp256k1::XOnlyPublicKey,
        created_at: u64,
        kind: u64,
        tags: &Vec<Vec<String>>,
        content: &String,
    ) -> secp256k1::Message {
        let mut formatted_tags = Self::format_tags(tags);
        // formatted_tags.retain(|c| !c.is_whitespace());

        let msg = format!(
            r#"[0,"{}",{},{},[{}],"{}"]"#,
            pubkey, created_at, kind, formatted_tags, content
        );
        secp256k1::Message::from_hashed_data::<secp256k1::hashes::sha256::Hash>(msg.as_bytes())
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

#[cfg(test)]
mod tests {
    use super::*;
    const TEST_SECRET: &str = "67c497012395ded1448b06f4bc55abaa74e1fe8d60c3f635c980547171fb24f9";

    // Valid signature for "nope" message with timestamp 1660659551
    // Generated with TEST_SECRET, commitment '[0,"1640facabe8bcf73f3b3f85ad60b55154276806cd65226b8cbda680fc9149995",1660659551,1,[],"nope"]'
    const NOPE_SIG: &str = "af372c8cb342d27d0c2363040b676aa7d68a0d028f09e73b567675f1beb15c29f07c970c7b65e3451f4fcc31e9c855534ad3d4e1a3af0d6bcdbab85c340df8bd";

    fn test_keypair(secret: &str) -> secp256k1::KeyPair {
        let secp = secp256k1::Secp256k1::new();
        secp256k1::KeyPair::from_seckey_str(&secp, secret).unwrap()
    }

    fn get_test_event(keypair: secp256k1::KeyPair) -> Event {
        Event::new(
            &keypair,
            1660656918,
            1,
            vec![
                vec![
                    "e".to_string(),
                    "012df9baa5377d7f2b29922249fe36e2fde5daab3060342b93235c9b6db444dc".to_string(),
                ],
                vec![
                    "p".to_string(),
                    "8b5a3bd6143fc0ad19c80886628097140b8dafd7adc1302444dff8d8645540f8".to_string(),
                ],
                vec!["random_tag".to_string(), "just testing here".to_string()],
            ],
            "Testing nostr-bot.".to_string(),
        )
    }

    #[test]
    fn new_event() {
        let keypair = test_keypair(TEST_SECRET);
        let event = get_test_event(keypair);

        // Check correct id was generated
        assert_eq!(
            event.id,
            "90ce185727fed0c5695e9e20d9c2130bd1d4b8776078016423420d6110d353fe"
        );

        let signature = secp256k1::schnorr::Signature::from_str(&event.sig).unwrap();
        let (x_only_public_key, _parity) = keypair.x_only_public_key();
        let message = Event::get_id(
            x_only_public_key,
            event.created_at,
            event.kind,
            &event.tags,
            &event.content,
        );

        // Check Event::get_id is generating correct ID
        assert_eq!(message.to_string(), event.id);

        // Verify the ID was signed with the TEST_SECRET
        let secp = secp256k1::Secp256k1::verification_only();
        assert!(!secp
            .verify_schnorr(&signature, &message, &x_only_public_key)
            .is_err());

        // Now let's put signature (valid but for different message) and see if it's revoked
        let mut event = event;
        event.sig = NOPE_SIG.to_string();
        let signature = secp256k1::schnorr::Signature::from_str(&event.sig).unwrap();

        // This should fail
        assert!(secp
            .verify_schnorr(&signature, &message, &x_only_public_key)
            .is_err());
    }

    #[test]
    fn valid_event() {
        let keypair = test_keypair(TEST_SECRET);
        let event = get_test_event(keypair);

        assert!(event.has_valid_sig());

        // Changing signature should make the event invalid
        let mut event = get_test_event(keypair);
        event.sig = NOPE_SIG.to_string();
        assert!(!event.has_valid_sig());

        // Now let's put some random values into the original Event and check that it becomes
        // invalid

        // Changing id
        let mut event = get_test_event(keypair);
        event.id = "9892364de48ac0e02cf2fc3c4ddb58f29721bd0024db06495a8f9396710dbe36".to_string();
        assert!(!event.has_valid_sig());

        let mut event = get_test_event(keypair);
        event.created_at = 123456789;
        assert!(!event.has_valid_sig());

        // Changing tags
        let mut event = get_test_event(keypair);
        event.tags = vec![vec!["random_tag".to_string(), "hohoho".to_string()]];
        assert!(!event.has_valid_sig());

        // Changing tags
        let mut event = get_test_event(keypair);
        event.tags = vec![vec!["random_tag".to_string(), "hohoho".to_string()]];
        assert!(!event.has_valid_sig());

        // Changing tags
        let mut event = get_test_event(keypair);
        event.content = "Difference content".to_string();
        assert!(!event.has_valid_sig());
    }
}
