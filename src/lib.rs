use log::debug;

mod bot;
mod network;
mod nostr;
mod utils;

pub use bot::Functor;
pub use network::Network;
pub use nostr::{format_reply, Event, EventNonSigned};

use bot::{Profile, Sender, SenderRaw};

pub type FunctorRaw<State> =
    dyn Fn(
        nostr::Event,
        State,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = nostr::EventNonSigned>>>;

pub type Commands<State> =
    std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, Functor<State>>>>;

pub type State<T> = std::sync::Arc<tokio::sync::Mutex<T>>;

#[macro_export]
macro_rules! wrap {
    ($x:expr) => {
        Box::new(|event, state| Box::pin($x(event, state)))
    };
}

pub struct Bot<State: Clone + Send + Sync> {
    keypair: secp256k1::KeyPair,
    relays: Vec<String>,
    network_type: network::Network,
    commands: Commands<State>,

    profile: Profile,

    sender: Sender,
    streams: Option<Vec<network::Stream>>,
}

impl<State: Clone + Send + Sync> Bot<State> {
    pub fn new(
        keypair: secp256k1::KeyPair,
        relays: Vec<String>,
        network_type: network::Network,
    ) -> Self {
        Bot {
            keypair,
            relays,
            network_type,
            commands: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            profile: Profile::new(),

            sender: std::sync::Arc::new(tokio::sync::Mutex::new(SenderRaw { sinks: vec![] })),
            streams: None,
        }
    }

    pub fn add_command(self, command: &str, cl: Functor<State>) -> Self {
        match self
            .commands
            .lock()
            .unwrap()
            .insert(command.to_string(), cl)
        {
            Some(_) => panic!("Failed to add command {}", command),
            None => {}
        }
        self
    }

    pub fn get_sender(&self) -> Sender {
        self.sender.clone()
    }

    pub fn set_name(mut self, name: &str) -> Self {
        self.profile.name = Some(name.to_string());
        self
    }

    pub fn set_about(mut self, about: &str) -> Self {
        self.profile.about = Some(about.to_string());
        self
    }

    pub fn set_picture(mut self, picture_url: &str) -> Self {
        self.profile.picture_url = Some(picture_url.to_string());
        self
    }

    pub fn set_intro_message(mut self, message: &str) -> Self {
        self.profile.intro_message = Some(message.to_string());
        self
    }

    pub async fn connect(&mut self) {
        debug!("Connecting to relays.");
        let (sinks, streams) = network::try_connect(&self.relays, &self.network_type).await;
        assert!(!sinks.is_empty() && !streams.is_empty());
        self.sender = std::sync::Arc::new(tokio::sync::Mutex::new(SenderRaw { sinks }));
        self.streams = Some(streams);
    }

    pub async fn run(&mut self, state: State) {
        if let None = self.streams {
            debug!("Running run() but there is no connection yet. Connecting now.");
            self.connect().await;
        }

        self.really_run(state).await;
    }
}

pub fn init_logger() {
    // let _start = std::time::Instant::now();
    env_logger::Builder::from_default_env()
        // .format(move |buf, rec| {
        // let t = start.elapsed().as_secs_f32();
        // writeln!(buf, "{:.03} [{}] - {}", t, rec.level(), rec.args())
        // })
        .init();
}

pub fn wrap_state<T>(gift: T) -> State<T> {
    std::sync::Arc::new(tokio::sync::Mutex::new(gift))
}
