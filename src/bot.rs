use futures_util::StreamExt;
use log::{debug, info, warn};
use rand::Rng;

use crate::Bot;
use crate::{network, nostr, utils};

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

pub type Commands<State> = std::sync::Arc<std::sync::Mutex<Vec<Command<State>>>>;

// Implementation of internal Bot methods
impl<State: Clone + Send + Sync> Bot<State> {
    pub(super) async fn really_run(&mut self, state: State) {
        set_profile(
            &self.keypair,
            self.sender.clone(),
            self.profile.name.clone(),
            self.profile.about.clone(),
            self.profile.picture_url.clone(),
        )
        .await;

        let (main_bot_tx, main_bot_rx) = tokio::sync::mpsc::channel::<nostr::Message>(64);

        let keypair = self.keypair;
        let sinks = self.sender.lock().await.sinks.clone();

        let streams = self.streams.take();
        if let Some(streams) = streams {
            for (id, stream) in streams.into_iter().enumerate() {
                let sink = sinks[id].clone();
                let main_bot_tx = main_bot_tx.clone();
                tokio::spawn(async move {
                    listen_relay(stream, sink, main_bot_tx, keypair).await;
                });
            }
        }

        if let Some(message) = &self.profile.intro_message {
            // Say hi
            let welcome = nostr::Event::new(
                &keypair,
                utils::unix_timestamp(),
                1,
                vec![],
                message.clone(),
            );

            info!("main bot is sending message \"{}\"", message);
            self.sender.lock().await.send(welcome.format()).await;
        };

        let commands = self.commands.clone();
        self.main_bot_listener(
            state.clone(),
            self.sender.clone(),
            main_bot_rx,
            &keypair,
            commands,
        )
        .await
    }

    pub async fn main_bot_listener(
        &self,
        state: State,
        sender: Sender,
        mut rx: NostrMessageReceiver,
        keypair: &secp256k1::KeyPair,
        commands: Commands<State>,
    ) {
        let mut handled_events = std::collections::HashSet::new();

        let bot_info = BotInfo {
            help: self.generate_help(),
            sender: self.sender.clone(),
        };

        info!("Main bot listener started.");
        while let Some(message) = rx.recv().await {
            let event_id = message.content.id.clone();
            if handled_events.contains(&event_id) {
                debug!("Event with id={} already handled, ignoring.", event_id);
                continue;
            }

            handled_events.insert(event_id);

            debug!("Handling {}", message.content.format());

            let command = message.content.content.clone();
            let words = command.split_whitespace().collect::<Vec<_>>();
            if words.len() == 0 {
                continue;
            }
            let command_part = words[0];

            let response = {
                let mut fallthrough_command = None;
                let mut functor = None;

                let commands = commands.lock().unwrap();

                for command in commands.iter() {
                    if command.prefix == "" {
                        fallthrough_command = Some(&command.functor);
                        continue;
                    }

                    if command_part.starts_with(&command.prefix) {
                        functor = Some(&command.functor);
                    }
                }

                let functor_to_call = if let Some(functor) = functor {
                    debug!("Found functor to run, going to run it.");
                    Some(functor)
                } else {
                    if let Some(fallthrough_command) = fallthrough_command {
                        debug!("Going to call fallthrough command \"\".");
                        Some(fallthrough_command)
                    } else {
                        debug!("Didn't find command >{}<, ignoring.", command_part);
                        None
                    }
                };

                if let Some(functor) = functor_to_call {
                    match functor {
                        FunctorType::Basic(functor) => {
                            Some((functor)(message.content, state.clone()).await)
                        }
                        FunctorType::Extra(functor) => {
                            Some((functor)(message.content, state.clone(), bot_info.clone()).await)
                        }
                    }
                } else {
                    None
                }
            };

            if let Some(response) = response {
                let response = response;
                sender
                    .lock()
                    .await
                    .send(response.sign(keypair).format())
                    .await;
            }
        }
    }

    fn generate_help(&self) -> String {
        let mut help = match self.profile.about.clone() {
            Some(about) => about,
            None => String::new(),
        };

        help.push_str("\n\nAvailable commands:\n");
        let commands = self.commands.lock().unwrap();
        for command in commands.iter() {
            let description = match &command.description {
                Some(description) => description,
                None => "",
            };
            let prefix = if command.prefix.len() > 0 {
                command.prefix.clone()
            } else {
                "\"\"".to_string()
            };
            help.push_str(&format!("{}...{}\n", prefix, description));
        }

        help
    }
}

#[derive(Clone)]
pub struct BotInfo {
    help: String,
    sender: Sender,
}

impl BotInfo {
    pub async fn connected_relays(&self) -> Vec<String> {
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

type NostrMessageReceiver = tokio::sync::mpsc::Receiver<nostr::Message>;
type NostrMessageSender = tokio::sync::mpsc::Sender<nostr::Message>;

pub struct Profile {
    pub name: Option<String>,
    pub about: Option<String>,
    pub picture_url: Option<String>,
    pub intro_message: Option<String>,
}

impl Profile {
    pub fn new() -> Self {
        Self {
            name: None,
            about: None,
            picture_url: None,
            intro_message: None,
        }
    }
}

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

pub async fn set_profile(
    keypair: &secp256k1::KeyPair,
    sender: Sender,
    name: Option<String>,
    about: Option<String>,
    picture_url: Option<String>,
) {
    info!(
        "main bot is settings name: \"{:?}\", about: \"{:?}\", picture_url: \"{:?}\"",
        name, about, picture_url
    );

    // Set profile
    let message = nostr::get_profile_event(name, about, picture_url)
        .sign(keypair)
        .format();

    sender.lock().await.send(message).await;
}

pub async fn request_subscription(keypair: &secp256k1::KeyPair, sink: network::Sink) {
    let random_string = rand::thread_rng()
        .sample_iter(rand::distributions::Alphanumeric)
        .take(64)
        .collect::<Vec<_>>();
    let random_string = String::from_utf8(random_string).unwrap();
    // Listen for my pubkey mentions
    network::send(
        format!(
            r##"["REQ", "{}", {{"#p": ["{}"], "kind": 1, "since": {}}} ]"##,
            random_string,
            keypair.x_only_public_key().0,
            utils::unix_timestamp(),
        ),
        sink,
    )
    .await;
}

async fn relay_listener(
    stream: network::Stream,
    sink: network::Sink,
    main_bot_tx: NostrMessageSender,
    main_bot_keypair: &secp256k1::KeyPair,
) {
    request_subscription(main_bot_keypair, sink.clone()).await;

    let listen = |message: Result<tungstenite::Message, tungstenite::Error>| async {
        let data = match message {
            Ok(data) => data,
            Err(error) => {
                info!("Stream read failed: {}", error);
                return;
            }
        };

        let data_str = data.to_string();
        debug!("Got message >{}< from {}.", data_str, stream.peer_addr);

        match serde_json::from_str::<nostr::Message>(&data.to_string()) {
            Ok(message) => {
                debug!(
                    "Sending message with event id={} to master bot",
                    message.content.id
                );
                match main_bot_tx.send(message).await {
                    Ok(_) => {}
                    Err(e) => panic!("Error sending message to main bot: {}", e),
                }
            }
            Err(e) => {
                debug!("Unable to parse message: {}", e);
            }
        }
    };

    match stream.stream {
        network::StreamType::Clearnet(stream) => {
            let f = stream.for_each(listen);
            f.await;
        }
        network::StreamType::Tor(stream) => {
            let f = stream.for_each(listen);
            f.await;
        }
    }
}

async fn listen_relay(
    stream: network::Stream,
    sink: network::Sink,
    main_bot_tx: NostrMessageSender,
    main_bot_keypair: secp256k1::KeyPair,
) {
    info!("Relay listener for {} started.", sink.peer_addr);
    let peer_addr = sink.peer_addr.clone();

    let network_type = match sink.clone().sink {
        network::SinkType::Clearnet(_) => network::Network::Clearnet,
        network::SinkType::Tor(_) => network::Network::Tor,
    };

    let mut stream = stream;
    let mut sink = sink;

    loop {
        relay_listener(stream, sink.clone(), main_bot_tx.clone(), &main_bot_keypair).await;
        let wait = std::time::Duration::from_secs(30);
        warn!(
            "Connection with {} lost, I will try to reconnect in {:?}",
            peer_addr, wait
        );

        // Reconnect
        loop {
            tokio::time::sleep(wait).await;
            let connection = network::get_connection(&peer_addr, &network_type).await;
            match connection {
                Ok((new_sink, new_stream)) => {
                    sink.update(new_sink.sink).await;
                    stream = new_stream;
                    break;
                }
                Err(_) => warn!(
                    "Relay listener is unable to reconnect to {}. Will try again in {:?}",
                    peer_addr, wait
                ),
            }
        }
    }
}

pub async fn help_command<State>(
    event: nostr::Event,
    _state: State,
    bot_info: BotInfo,
) -> nostr::EventNonSigned {
    nostr::format_reply(event, bot_info.help)
}
