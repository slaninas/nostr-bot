use log::debug;
use std::future::Future;

mod bot;
mod network;
mod nostr;
pub mod utils;

pub use network::Network;
pub use nostr::{format_reply, tags_for_reply, Event, EventNonSigned};

pub type State<T> = std::sync::Arc<tokio::sync::Mutex<T>>;

// Sender
pub type Sender = std::sync::Arc<tokio::sync::Mutex<SenderRaw>>;

pub struct SenderRaw {
    pub sinks: Vec<network::Sink>,
}

impl SenderRaw {
    pub async fn send(&self, message: String) {
        network::send_to_all(message, self.sinks.clone()).await;
    }

    pub fn add(&mut self, sink: network::Sink) {
        self.sinks.push(sink);
    }
}

// Functors
pub type FunctorRaw<State> =
    dyn Fn(
        nostr::Event,
        State,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = nostr::EventNonSigned>>>;

pub type Functor<State> = Box<FunctorRaw<State>>;

pub type FunctorExtraRaw<State> =
    dyn Fn(
        nostr::Event,
        State,
        BotInfo,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = nostr::EventNonSigned>>>;

pub type FunctorExtra<State> = Box<FunctorExtraRaw<State>>;

pub enum FunctorType<State> {
    Basic(Functor<State>),
    Extra(FunctorExtra<State>),
}

// Commands
pub struct Command<State: Clone + Send + Sync> {
    pub prefix: String,
    pub description: Option<String>,
    pub functor: FunctorType<State>,
}

impl<State: Clone + Send + Sync> Command<State> {
    pub fn new(prefix: &str, functor: FunctorType<State>) -> Self {
        Self {
            prefix: prefix.to_string(),
            description: None,
            functor,
        }
    }

    pub fn desc(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }
}


// Macros for easier wrapping
#[macro_export]
macro_rules! wrap {
    ($functor:expr) => {
        FunctorType::Basic(Box::new(|event, state| Box::pin($functor(event, state))))
    };
}

#[macro_export]
macro_rules! wrap_extra {
    ($functor:expr) => {
        FunctorType::Extra(Box::new(|event, state, text| {
            Box::pin($functor(event, state, text))
        }))
    };
}

// Bot stuff
pub struct Bot<State: Clone + Send + Sync> {
    keypair: secp256k1::KeyPair,
    relays: Vec<url::Url>,
    network_type: network::Network,

    commands: bot::Commands<State>,
    state: State,

    profile: bot::Profile,

    sender: Sender, // TODO: Use Option
    streams: Option<Vec<network::Stream>>,
    to_spawn: Vec<Box<dyn std::future::Future<Output = ()> + Send + Unpin>>,

}

impl<State: Clone + Send + Sync + 'static> Bot<State> {
    pub fn new(
        keypair: secp256k1::KeyPair,
        relays: Vec<url::Url>,
        network_type: network::Network,
        state: State,
    ) -> Self {
        Bot {
            keypair,
            relays,
            network_type,

            commands: std::sync::Arc::new(std::sync::Mutex::new(vec![])),
            state,

            profile: bot::Profile::new(),

            sender: std::sync::Arc::new(tokio::sync::Mutex::new(SenderRaw { sinks: vec![] })),
            streams: None,
            to_spawn: vec![],
        }
    }

    pub fn name(mut self, name: &str) -> Self {
        self.profile.name = Some(name.to_string());
        self
    }

    pub fn about(mut self, about: &str) -> Self {
        self.profile.about = Some(about.to_string());
        self
    }

    pub fn picture(mut self, picture_url: &str) -> Self {
        self.profile.picture_url = Some(picture_url.to_string());
        self
    }

    pub fn intro_message(mut self, message: &str) -> Self {
        self.profile.intro_message = Some(message.to_string());
        self
    }

    pub fn help(self) -> Self {
        self.commands
            .lock()
            .unwrap()
            .push(Command::new("!help", wrap_extra!(bot::help_command)).desc("Show this help."));
        self
    }

    pub fn command(self, command: Command<State>) -> Self {
        self.commands.lock().unwrap().push(command);
        self
    }

    pub fn spawn(mut self, fut: impl Future<Output = ()> + Unpin + Send + 'static) -> Self {
        self.to_spawn.push(Box::new(fut));
        self
    }

    pub fn sender(mut self, sender: Sender) -> Self {
        self.sender = sender;
        self
    }

    pub async fn run(&mut self) {
        if let None = self.streams {
            debug!("Running run() but there is no connection yet. Connecting now.");
            self.connect().await;
        }

        self.really_run(self.state.clone()).await;
    }
}

// BotInfo - Bot proxy
#[derive(Clone)]
pub struct BotInfo {
    help: String,
    sender: Sender,
}

impl BotInfo {
    pub async fn connected_relays(&self) -> Vec<url::Url> {
        let sender = self.sender.clone();
        let sinks = sender.lock().await.sinks.clone();

        let mut results = vec![];
        for relay in sinks {
            let peer_addr = relay.peer_addr.clone();
            if network::ping(relay).await {
                results.push(peer_addr);
            }
        }

        results
    }
}

// Misc
pub fn new_sender() -> Sender {
    std::sync::Arc::new(tokio::sync::Mutex::new(SenderRaw { sinks: vec![] }))
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
