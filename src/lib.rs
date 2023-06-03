#![doc = include_str!("../README.md")]

// TODO: Proper error handling withou unwraps + enable unwrap check in clippy

use log::debug;
use std::{future::Future, collections::HashMap};

mod bot;
mod network;
mod nostr;
mod utils;

pub extern crate log;
pub extern crate secp256k1;
pub extern crate tokio;

pub use network::ConnectionType;
pub use nostr::{get_reply, tags_for_reply, Event, EventNonSigned, Subscription};
pub use utils::{keypair_from_secret, unix_timestamp};

pub type State<T> = std::sync::Arc<tokio::sync::Mutex<T>>;

/// Just a wrapper so the [SenderRaw] can be shared.
pub type Sender = std::sync::Arc<tokio::sync::Mutex<SenderRaw>>;

/// Holds sinks which can be used to send messages to relays.
pub struct SenderRaw {
    pub sinks: Vec<network::Sink>,
}

impl SenderRaw {
    /// Sends `event` to all sinks it holds.
    pub async fn send(&self, event: nostr::Event) {
        network::send_to_all(&event.format(), self.sinks.clone()).await;
    }

    /// Sends `message` to all sinks it holds.
    pub async fn send_str(&self, message: &str) {
        network::send_to_all(message, self.sinks.clone()).await;
    }

    /// Adds new sink.
    pub fn add(&mut self, sink: network::Sink) {
        self.sinks.push(sink);
    }
}

// Functors
pub type FunctorTestRaw<State> =
    fn(nostr::Event, State) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>;

pub type FunctorTest<State> = Box<FunctorTestRaw<State>>;

pub type FunctorRaw<State> =
    fn(
        nostr::Event,
        State,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = nostr::EventNonSigned> + Send>>;

pub type Functor<State> = Box<FunctorRaw<State>>;

pub type FunctorExtraRaw<State> =
    fn(
        nostr::Event,
        State,
        BotInfo,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = nostr::EventNonSigned> + Send>>;

pub type FunctorExtra<State> = Box<FunctorExtraRaw<State>>;

pub type FunctorOptionRaw<State> =
    fn(
        nostr::Event,
        State,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<nostr::EventNonSigned>> + Send>>;

pub type FunctorOption<State> = Box<FunctorOptionRaw<State>>;

/// Describes various functor types.
///
/// You should not need to use it directly, use [wrap] or [wrap_extra] macros instead.
#[derive(Clone)]
pub enum FunctorType<State> {
    Basic(Functor<State>),
    Extra(FunctorExtra<State>),
    Option(FunctorOption<State>)
}

// Commands

/// Holds info about invocable commands.
pub struct Command<State: Clone + Send + Sync> {
    pub prefix: String,
    pub description: Option<String>,
    functor: FunctorType<State>,
}

impl<State: Clone + Send + Sync> Command<State> {
    /// Create new description.
    /// * `prefix` Prefix which will be used for commands matching.
    /// * `functor` Functor that is run when bot finds matching command.
    pub fn new(prefix: &str, functor: FunctorType<State>) -> Self {
        Self {
            prefix: prefix.to_string(),
            description: None,
            functor,
        }
    }

    /// Add description for command.
    ///
    /// This is used by [Bot::help()] when generating !help command.
    pub fn description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }
}

pub struct Handler<State: Clone + Send + Sync> {
    pub subs: Vec<nostr::Subscription>,
    functor: FunctorType<State>
}

impl<State: Clone + Send + Sync> Handler<State> {
    pub fn new(sub: Subscription, functor: FunctorType<State>) -> Self {
        match functor {
            FunctorType::Option(_) => 
            Self {
                subs: vec![sub],
                functor,
            },
            _ => panic!("Expected FunctorType::Option")
        }
        
        
    }
}

pub struct Actor<State: Clone + Send + Sync> {
    pub subs: Vec<nostr::Subscription>,
    pub handler: Handler<State>,
}

// Macros for easier wrapping

/// Wraps your functor so it can be passed to the bot.
#[macro_export]
macro_rules! wrap {
    ($functor:expr) => {
        FunctorType::Basic(Box::new(|event, state| Box::pin($functor(event, state))))
    };
}

/// Wraps your functor so it can be passed to the bot.
#[macro_export]
macro_rules! wrap_extra {
    ($functor:expr) => {
        FunctorType::Extra(Box::new(|event, state, text| {
            Box::pin($functor(event, state, text))
        }))
    };
}

#[macro_export]
macro_rules! wrap_option {
    ($functor:expr) => {
        FunctorType::Option(Box::new(|event, state| Box::pin($functor(event, state))))
    };
}

// Bot stuff

/// Main sctruct that holds every data necessary to run a bot.
/// The core purpose is to take care of the relays connections and listening for commands.
pub struct Bot<State: Clone + Send + Sync> {
    keypair: secp256k1::KeyPair,
    relays: Vec<String>,
    connection_type: ConnectionType,
    proxy_addr: Option<String>,

    user_commands: bot::UserCommands<State>,
    commands: bot::Commands<State>,
    state: State,

    bot_handlers: bot::BotHandlers<State>,
    handlers: bot::Handlers<State>,

    subscription_ids: HashMap<String, FunctorType<State>>,

    profile: bot::Profile,

    sender: Sender, // TODO: Use Option
    streams: Option<Vec<network::Stream>>,
    to_spawn: Vec<Box<dyn std::future::Future<Output = ()> + Send + Unpin>>,
}

impl<State: Clone + Send + Sync + 'static> Bot<State> {
    /// Basic initialization of the bot.
    /// * `keypair` Key pair that  will be used by the bot to sign messages.
    /// * `relays` List of relays to which the bot will connect to.
    /// * `state` Shared object that will be passed to invoked commands, see [Bot::command()].
    pub fn new(keypair: secp256k1::KeyPair, relays: Vec<&str>, state: State) -> Self {
        Bot {
            keypair,
            relays: relays.iter().map(|s| s.to_string()).collect(),
            connection_type: ConnectionType::Direct,
            proxy_addr: None,

            user_commands: vec![],
            commands: std::sync::Arc::new(tokio::sync::Mutex::new(vec![])),

            bot_handlers: vec![],
            handlers: std::sync::Arc::new(tokio::sync::Mutex::new(vec![])),
            state,
            subscription_ids: HashMap::new(),

            profile: bot::Profile::new(),

            sender: std::sync::Arc::new(tokio::sync::Mutex::new(SenderRaw { sinks: vec![] })),
            streams: None,
            to_spawn: vec![],
        }
    }

    /// Sets bot's name.
    /// * `name` After connecting to relays this name will be send inside set_metadata kind 0 event, see
    /// <https://github.com/nostr-protocol/nips/blob/master/01.md#basic-event-kinds>.
    pub fn name(mut self, name: &str) -> Self {
        self.profile.name = Some(name.to_string());
        self
    }

    /// Sets bot's about info
    /// * `about` After connecting to relays this info will be send inside set_metadata kind 0 event, see
    /// <https://github.com/nostr-protocol/nips/blob/master/01.md#basic-event-kinds>.
    /// Also, this is used when generating !help command, see [Bot::help()].
    pub fn about(mut self, about: &str) -> Self {
        self.profile.about = Some(about.to_string());
        self
    }

    /// Set bot's profile picture
    /// * `picture_url` After connecting to relays this will be send inside set_metadata kind 0 event, see
    /// <https://github.com/nostr-protocol/nips/blob/master/01.md#basic-event-kinds>.
    pub fn picture(mut self, picture_url: &str) -> Self {
        self.profile.picture_url = Some(picture_url.to_string());
        self
    }

    /// Says hello.
    /// * `message` This message will be send when bot connects to a relay.
    pub fn intro_message(mut self, message: &str) -> Self {
        self.profile.intro_message = Some(message.to_string());
        self
    }

    /// Generates "manpage".
    ///
    /// This adds !help command.
    /// When invoked it shows info about bot set in [Bot::about()] and
    /// auto-generated list of available commands.
    pub fn help(mut self) -> Self {
        self.user_commands.push(
            Command::new("!help", wrap_extra!(bot::help_command)).description("Show this help."),
        );
        self
    }

    /// Registers `command`.
    ///
    /// When someone replies to a bot, the bot goes through all registered commands and when it
    /// finds match it invokes given functor.
    pub fn command(mut self, command: Command<State>) -> Self {
        self.user_commands.push(command);
        self
    }

    /// Registers 'handler'.
    /// 
    /// When a message comes in for a subscription
    pub fn handler(mut self, handler: Handler<State>) -> Self {
        for sub in & handler.subs {
            self.subscription_ids.insert(sub.id.to_string(), handler.functor.clone());
        } 
        self.bot_handlers.push(handler);
        self
    }

    pub fn subscriptions(self) -> Vec<Subscription> {
        let mut subs = vec![];
        for handler in self.bot_handlers {
            for sub in handler.subs {
                subs.push(sub);
            }
        }
        subs
    }
    /// Adds a task that will be spawned [tokio::spawn].
    /// * `future` Future is saved and the task is spawned when [Bot::run] is called and bot
    /// connects to the relays.
    ///
    /// # Example
    /// ```rust
    /// // Sending message every 60 seconds
    /// #[tokio::main]
    /// async fn main() {
    ///     nostr_bot::init_logger();
    ///
    ///     let keypair = nostr_bot::keypair_from_secret(
    ///         // Your secret goes here
    ///     );
    ///     let relays = vec![
    ///         // List of relays goes here
    ///     ];
    ///
    ///     let sender = nostr_bot::new_sender();
    ///     // Empty state just for example sake
    ///     let state = nostr_bot::wrap_state(());
    ///
    ///     // Tip: instead of capturing the sender you can capture your state
    ///     // and update it here regularly
    ///     let alive = {
    ///         let sender = sender.clone();
    ///         async move {
    ///             loop {
    ///                 let event = nostr_bot::EventNonSigned {
    ///                     created_at: nostr_bot::unix_timestamp(),
    ///                     kind: 1,
    ///                     content: "I'm still alive.".to_string(),
    ///                     tags: vec![],
    ///                 }
    ///                 .sign(&keypair);
    ///                 sender.lock().await.send(event).await;
    ///                 tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    ///             }
    ///         }
    ///     };
    ///
    ///     nostr_bot::Bot::new(keypair, relays, state)
    ///         // You have to set the sender here so that the alive future
    ///         // and the bot share the same one
    ///         .sender(sender)
    ///         .spawn(Box::pin(alive))
    ///         .run()
    ///         .await;
    /// }
    /// ```
    pub fn spawn(mut self, future: impl Future<Output = ()> + Unpin + Send + 'static) -> Self {
        self.to_spawn.push(Box::new(future));
        self
    }

    /// Sets sender.
    ///
    /// It can be used together with [Bot::spawn] to make the bot send messages outside it's
    /// command responses.
    /// * `sender` Sender that will be used by bot to send nostr messages to relays.
    pub fn sender(mut self, sender: Sender) -> Self {
        self.sender = sender;
        self
    }

    /// Tells the bot to use socks5 proxy instead of direct connection to the internet
    /// for communication with relays.
    ///
    /// If you need anonymity please **check yourself there are no leaks**.
    /// * `proxy_addr` Address of the proxy including port, e.g. `127.0.0.1:9050`.
    pub fn use_socks5(mut self, proxy_addr: &str) -> Self {
        self.connection_type = ConnectionType::Socks5;
        self.proxy_addr = Some(proxy_addr.to_string());
        self
    }

    /// Connects to relays, set bot's profile, send message if set using [Bot::intro_message],
    /// spawn tasks if given prior by [Bot::spawn()] and listen to commands.
    pub async fn run(&mut self) {
        let mut user_commands = vec![];
        std::mem::swap(&mut user_commands, &mut self.user_commands);
        *self.commands.lock().await = user_commands;
        let mut bot_handlers = vec![];
        std::mem::swap(&mut bot_handlers, &mut self.bot_handlers);
        *self.handlers.lock().await = bot_handlers;
        if self.streams.is_none() {
            debug!("Running run() but there is no connection yet. Connecting now.");
            self.connect().await;
        }

        self.really_run().await;
    }
}

/// Struct for holding informations about bot.
#[derive(Clone)]
pub struct BotInfo {
    help: String,
    sender: Sender,
}

impl BotInfo {
    /// Returns list of relays to which the bot is able to send a message.
    pub async fn connected_relays(&self) -> Vec<String> {
        let sender = self.sender.clone();
        let sinks = sender.lock().await.sinks.clone();

        let mut results = vec![];
        for relay in sinks {
            let peer_addr = relay.peer_addr.clone();
            if network::send_message(relay, tungstenite::Message::Ping(vec![])).await {
                results.push(peer_addr);
            }
        }

        results
    }
}

// Misc

/// Returns new (empty) [Sender] which can be used to send messages to relays.
pub fn new_sender() -> Sender {
    std::sync::Arc::new(tokio::sync::Mutex::new(SenderRaw { sinks: vec![] }))
}

/// Init [env_logger].
pub fn init_logger() {
    // let _start = std::time::Instant::now();
    env_logger::Builder::from_default_env()
        // .format(move |buf, rec| {
        // let t = start.elapsed().as_secs_f32();
        // writeln!(buf, "{:.03} [{}] - {}", t, rec.level(), rec.args())
        // })
        .init();
}

/// Wraps given object into Arc Mutex.
pub fn wrap_state<T>(gift: T) -> State<T> {
    std::sync::Arc::new(tokio::sync::Mutex::new(gift))
}
