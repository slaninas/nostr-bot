/*!
Do you want to run your own nostr bot? You've come to the right place.
This crate makes it easy for you to implement bot that reacts to the nostr events.

# Usage
This crate is on [crates.io](https://crates.io/crates/nostr-bot) and can be
used by adding `nostr-bot` to your dependencies in your project's `Cargo.toml`.

Be aware that this crate is still being developed and until 1.0 is out there may be API breaking
even in MINOR (see [SemVer](https://semver.org/)) releases, PATCHES should be compatible
so If you want to make sure the API stays compatible with your code commit to a specific MINOR version:

```toml
[dependencies]
nostr-bot = "0.1"

```
Go to [Bot] to see main struct of the crate.

# Example
```rust
// Bot that reacts to '!yes', '!no' and '!results' commands.
use nostr_bot::*;

// Your struct that will be passed to the commands responses
struct Votes {
    question: String,
    yes: u64,
    no: u64,
}

type State = nostr_bot::State<Votes>;

fn format_results(question: &str, votes: &Votes) -> String {
    format!(
        "{}\n------------------\nyes: {}\nno:  {}",
        question, votes.yes, votes.no
    )
}

// Command responses, you are getting nostr event and shared state as
// arguments and you are supposed to return non-signed event
// which is then signed using the bot's key and send to relays
async fn yes(event: Event, state: State) -> EventNonSigned {
    let mut votes = state.lock().await;
    votes.yes += 1;

    // Use given formatted voting results and use it to create new event
    // that is a reply to the incoming command
    get_reply(event, format_results(&votes.question, &votes))
}

async fn no(event: Event, state: State) -> EventNonSigned {
    let mut votes = state.lock().await;
    votes.no += 1;
    get_reply(event, format_results(&votes.question, &votes))
}

async fn results(event: Event, state: State) -> EventNonSigned {
    let votes = state.lock().await;
    get_reply(event, format_results(&votes.question, &votes))
}

#[tokio::main]
async fn main() {
    init_logger();

    let relays = vec![
        // url::Url::parse("wss://nostr-pub.wellorder.net").unwrap(),
        // url::Url::parse("wss://relay.damus.io").unwrap(),
        // url::Url::parse("wss://relay.nostr.info").unwrap(),
        url::Url::parse("ws://192.168.1.103:8889").unwrap(),
        url::Url::parse("ws://192.168.1.103:8890").unwrap(),
    ];

    let keypair = utils::keypair_from_secret(
        // Your secret goes here
        "0000000000000000000000000000000000000000000000000000000000000001",
    );

    let question = String::from("Do you think Pluto should be a planet?");

    // Wrap your object into Arc<Mutex> so it can be shared among command handlers
    let shared_state = nostr_bot::wrap_state(Votes {
        question: question.clone(),
        yes: 0,
        no: 0,
    });

    // Setup the bot - add info about it, add commands and then run it
    Bot::new(keypair, relays, shared_state)
        // You don't have to set these but then the bot will have incomplete profile info :(
        .name("poll_bot")
        .about("Just a bot.")
        .picture("https://thumbs.dreamstime.com/z/poll-survey-results-voting-election-\
                  opinion-word-red-d-letters-pie-chart-to-illustrate-opinions-61587174.jpg")
        .intro_message(&question)
        // You don't have to specify any command but then what will the bot do? Nothing.
        .command(Command::new("!yes", wrap!(yes)))
        .command(Command::new("!no", wrap!(no)))
        .command(Command::new("!results", wrap!(results)))
        // And finally run it
        .run().await;
}
```

*/

use log::debug;
use std::future::Future;

mod bot;
mod network;
mod nostr;
mod utils;

pub use utils::{unix_timestamp, keypair_from_secret};
pub use network::ConnectionType;
pub use nostr::{get_reply, tags_for_reply, Event, EventNonSigned};

pub type State<T> = std::sync::Arc<tokio::sync::Mutex<T>>;

// Sender
pub type Sender = std::sync::Arc<tokio::sync::Mutex<SenderRaw>>;

/// Holds sinks which can be used to send messages to relays
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

/// Describes various functor types.
///
/// You should not need to use it directly, use [wrap] or [wrap_extra] macros instead.
pub enum FunctorType<State> {
    Basic(Functor<State>),
    Extra(FunctorExtra<State>),
}

// Commands

/// Holds info about invocable commands
pub struct Command<State: Clone + Send + Sync> {
    pub prefix: String,
    pub description: Option<String>,
    functor: FunctorType<State>,
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

/// Wraps your functor so it can be passed to the bot
#[macro_export]
macro_rules! wrap {
    ($functor:expr) => {
        FunctorType::Basic(Box::new(|event, state| Box::pin($functor(event, state))))
    };
}

/// Wraps your functor so it can be passed to the bot
#[macro_export]
macro_rules! wrap_extra {
    ($functor:expr) => {
        FunctorType::Extra(Box::new(|event, state, text| {
            Box::pin($functor(event, state, text))
        }))
    };
}

// Bot stuff

/// Main sctruct that holds every data necessary to run a bot
pub struct Bot<State: Clone + Send + Sync> {
    keypair: secp256k1::KeyPair,
    relays: Vec<url::Url>,
    connection_type: ConnectionType,
    proxy_addr: Option<url::Url>,

    user_commands: bot::UserCommands<State>,
    commands: bot::Commands<State>,
    state: State,

    profile: bot::Profile,

    sender: Sender, // TODO: Use Option
    streams: Option<Vec<network::Stream>>,
    to_spawn: Vec<Box<dyn std::future::Future<Output = ()> + Send + Unpin>>,
}

impl<State: Clone + Send + Sync + 'static> Bot<State> {
    /// Basic initialization of the bot
    /// * `keypair` This keypair will be used by the bot to sign messages
    /// * `relays` List of relays to which the bot will connect to
    /// * `state` Shared object that will be passed to invoked commands, see [Bot::command()]
    pub fn new(keypair: secp256k1::KeyPair, relays: Vec<url::Url>, state: State) -> Self {
        Bot {
            keypair,
            relays,
            connection_type: ConnectionType::Direct,
            proxy_addr: None,

            user_commands: vec![],
            commands: std::sync::Arc::new(tokio::sync::Mutex::new(vec![])),
            state,

            profile: bot::Profile::new(),

            sender: std::sync::Arc::new(tokio::sync::Mutex::new(SenderRaw { sinks: vec![] })),
            streams: None,
            to_spawn: vec![],
        }
    }

    /// Sets bot's name
    /// * `name` After connecting to relays this name will be send inside set_metadata kind 0 event, see
    /// <https://github.com/nostr-protocol/nips/blob/master/01.md#basic-event-kinds>
    pub fn name(mut self, name: &str) -> Self {
        self.profile.name = Some(name.to_string());
        self
    }

    /// Sets bot's about info
    /// * `about` After connecting to relays this info will be send inside set_metadata kind 0 event, see
    /// <https://github.com/nostr-protocol/nips/blob/master/01.md#basic-event-kinds>.
    /// Also, this is used when generating !help command, see [Bot::help()]
    pub fn about(mut self, about: &str) -> Self {
        self.profile.about = Some(about.to_string());
        self
    }

    /// Set bot's profile picture
    /// * `picture_url` After connecting to relays this will be send inside set_metadata kind 0 event, see
    /// <https://github.com/nostr-protocol/nips/blob/master/01.md#basic-event-kinds>
    pub fn picture(mut self, picture_url: &str) -> Self {
        self.profile.picture_url = Some(picture_url.to_string());
        self
    }

    /// Says hello
    /// * `message` This message will be send when bot connects to a relay
    pub fn intro_message(mut self, message: &str) -> Self {
        self.profile.intro_message = Some(message.to_string());
        self
    }

    /// Generates "manpage"
    ///
    /// This adds !help command.
    /// When invoked it shows info about bot set in [Bot::about()] and
    /// auto-generated list of available commands.
    pub fn help(mut self) -> Self {
        self.user_commands
            .push(Command::new("!help", wrap_extra!(bot::help_command)).desc("Show this help."));
        self
    }

    /// Registers `command`.
    ///
    /// When someone replies to a bot, the bot goes through all registered commands and when it
    /// finds match it invokes given functor
    pub fn command(mut self, command: Command<State>) -> Self {
        self.user_commands.push(command);
        self
    }

    /// Adds a task that will be spawned [tokio::spawn]
    /// * `future` Future is saved and the task is spawned when [Bot::run] is called and bot
    /// connects to the relays
    pub fn spawn(mut self, future: impl Future<Output = ()> + Unpin + Send + 'static) -> Self {
        self.to_spawn.push(Box::new(future));
        self
    }

    /// Sets sender
    /// * `sender` Sender that will be used by bot to send nostr messages to relays
    pub fn sender(mut self, sender: Sender) -> Self {
        self.sender = sender;
        self
    }

    /// Tells the bot to use socks5 proxy instead of direct connection to the internet
    /// * `proxy_addr` Address of the proxy including port, e.g. `127.0.0.1:9050`
    pub fn use_socks5(mut self, proxy_addr: url::Url) -> Self {
        self.connection_type = ConnectionType::Socks5;
        self.proxy_addr = Some(proxy_addr);
        self
    }

    /// Connects to relays, set bot's profile, send message if set using [Bot::intro_message],
    /// spawn tasks if given prior by [Bot::spawn()] and listern to commandss
    pub async fn run(&mut self) {
        let mut user_commands = vec![];
        std::mem::swap(&mut user_commands, &mut self.user_commands);
        *self.commands.lock().await = user_commands;
        if self.streams.is_none() {
            debug!("Running run() but there is no connection yet. Connecting now.");
            self.connect().await;
        }

        self.really_run(self.state.clone()).await;
    }
}

/// Struct for holding informations about bot
#[derive(Clone)]
pub struct BotInfo {
    help: String,
    sender: Sender,
}

impl BotInfo {
    /// Returns list of relays to which the bot is able to send a message
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

/// Returns new (empty) [Sender] which can be used to send messages to relays
pub fn new_sender() -> Sender {
    std::sync::Arc::new(tokio::sync::Mutex::new(SenderRaw { sinks: vec![] }))
}

/// Init [env_logger]
pub fn init_logger() {
    // let _start = std::time::Instant::now();
    env_logger::Builder::from_default_env()
        // .format(move |buf, rec| {
        // let t = start.elapsed().as_secs_f32();
        // writeln!(buf, "{:.03} [{}] - {}", t, rec.level(), rec.args())
        // })
        .init();
}

/// Wraps given object into Arc Mutex
pub fn wrap_state<T>(gift: T) -> State<T> {
    std::sync::Arc::new(tokio::sync::Mutex::new(gift))
}
