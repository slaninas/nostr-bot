use log::warn;

#[derive(Clone)]
pub struct Config {
    pub name: String,
    pub about: String,
    pub picture_url: String,
    pub hello_message: String,
    pub secret: String,
    pub refresh_interval_secs: u64,
    pub relays: Vec<String>,
    pub max_follows: usize,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        fmt.debug_struct("Config")
            .field("name", &self.name)
            .field("about", &self.about)
            .field("picture_url", &self.picture_url)
            .field("hello_message", &self.hello_message)
            .field("secret", &"***")
            .field("refresh_interval_secs", &self.refresh_interval_secs)
            .field("relays", &self.relays)
            .field("max_follows", &self.max_follows)
            .finish()
    }
}

pub fn parse_config(path: &std::path::Path) -> Config {
    let get_value = |line: String| {
        let mut value = line.split('=').collect::<Vec<_>>()[1].to_string();
        if value.starts_with('"') && value.ends_with('"') {
            value = value[1..value.len() - 1].to_string();
        }
        value
    };

    let content = std::fs::read_to_string(path).expect("Config reading failed.");

    let mut name = String::from("");
    let mut about = String::from("");
    let mut picture_url = String::from("");
    let mut secret = String::new();
    let mut hello_message = String::new();
    let mut refresh_interval_secs = 0;
    let mut relays = Vec::new();
    let mut max_follows = 0;

    for line in content.lines() {
        let line = line.to_string();

        if line.starts_with("name") {
            name = get_value(line);
        } else if line.starts_with("about") {
            about = get_value(line);
        } else if line.starts_with("picture_url") {
            picture_url = get_value(line);
        } else if line.starts_with("hello_message") {
            hello_message = get_value(line);
        } else if line.starts_with("secret") {
            secret = get_value(line);
        } else if line.starts_with("refresh_interval_secs") {
            refresh_interval_secs = get_value(line)
                .parse::<u64>()
                .expect("Failed to parse the refresh interval.");
        } else if line.starts_with("addrelay") {
            relays.push(get_value(line));
        } else if line.starts_with("max_follows") {
            max_follows = get_value(line).parse::<usize>().expect("Can't parse value");
        } else if line.starts_with('#') || line.is_empty() {
            // Ignoring comments and blank lines
        } else {
            warn!("Unknown config line >{}<", line);
        }
    }

    assert!(!name.is_empty());
    assert!(!about.is_empty());
    assert!(!picture_url.is_empty());
    assert!(!secret.is_empty());
    assert!(!hello_message.is_empty());
    assert!(refresh_interval_secs > 0);
    assert!(!relays.is_empty());
    assert!(max_follows > 0);

    Config {
        name,
        about,
        picture_url,
        secret,
        hello_message,
        refresh_interval_secs,
        relays,
        max_follows,
    }
}

pub fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

pub fn get_random_keypair() -> secp256k1::KeyPair {
    let secp = secp256k1::Secp256k1::new();
    let secret = secp256k1::SecretKey::new(&mut rand::thread_rng());
    secret.keypair(&secp)
}
