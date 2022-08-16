use futures_util::StreamExt;
use log::{debug, info, warn};
use rand::Rng;
use std::fmt::Write;

use crate::*;

pub(super) type UserCommands<State> = Vec<Command<State>>;
pub(super) type Commands<State> = std::sync::Arc<tokio::sync::Mutex<UserCommands<State>>>;

// Implementation of internal Bot methods

impl<State: Clone + Send + Sync> Bot<State> {
    pub(super) async fn connect(&mut self) {
        debug!("Connecting to relays.");
        let (sinks, streams) =
            network::try_connect(&self.relays, &self.connection_type, &self.proxy_addr).await;
        assert!(!sinks.is_empty() && !streams.is_empty());
        // TODO: Check is sender isn't filled already
        *self.sender.lock().await = SenderRaw { sinks };
        self.streams = Some(streams);
    }

    pub(super) async fn really_run(&mut self) {
        // Ping relays every 30 seconds. This seems to be necessary at the moment to keep the
        // connection open for more than a minute. Some relays are sending pings and this lib
        // responds with pongs but some relays are closing connections without pinging first
        {
            let sender = self.sender.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    let sinks = sender.lock().await.sinks.clone();
                    for sink in sinks {
                        network::send_message(sink, tungstenite::Message::Ping(vec![])).await;
                    }
                }
            });
        }
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
                let proxy_addr = self.proxy_addr.clone();
                let sink = sinks[id].clone();
                let main_bot_tx = main_bot_tx.clone();

                tokio::spawn(async move {
                    listen_relay(stream, sink, main_bot_tx, keypair, proxy_addr).await;
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
            self.sender.lock().await.send(welcome).await;
        };

        let mut to_spawn = vec![];
        std::mem::swap(&mut to_spawn, &mut self.to_spawn);

        for fut in to_spawn {
            tokio::spawn(fut);
        }

        self.main_bot_listener(main_bot_rx, &keypair).await
    }

    pub(super) async fn main_bot_listener(
        &self,
        mut rx: NostrMessageReceiver,
        keypair: &secp256k1::KeyPair,
    ) {
        let mut handled_events = std::collections::HashSet::new();

        let bot_info = BotInfo {
            help: self.generate_help().await,
            sender: self.sender.clone(),
        };

        info!("Main bot listener started.");
        while let Some(message) = rx.recv().await {

            let event_id = message.content.id.clone();
            if !message.content.has_valid_sig() {
                warn!("Signature for event with id={} is not valid, ignoring.", event_id);
                continue;
            }

            if handled_events.contains(&event_id) {
                debug!("Event with id={} already handled, ignoring.", event_id);
                continue;
            }

            handled_events.insert(event_id);

            debug!("Handling {}", message.content.format());

            let response = self
                .execute_command(message.content, bot_info.clone())
                .await;

            if let Some(response) = response {
                let response = response;
                self.sender.lock().await.send(response.sign(keypair)).await;
            }
        }
    }

    async fn generate_help(&self) -> String {
        let mut help = match self.profile.about.clone() {
            Some(about) => about,
            None => String::new(),
        };

        help.push_str("\n\nAvailable commands:\n");
        let commands = self.commands.lock().await;
        for command in commands.iter() {
            let description = match &command.description {
                Some(description) => description,
                None => "",
            };
            let prefix = if !command.prefix.is_empty() {
                command.prefix.clone()
            } else {
                "\"\"".to_string()
            };
            writeln!(help, "{}...{}", prefix, description).unwrap();
        }

        help
    }

    async fn execute_command(
        &self,
        command_event: Event,
        bot_info: BotInfo,
    ) -> Option<EventNonSigned> {
        let command = command_event.content.clone();
        let words = command.split_whitespace().collect::<Vec<_>>();
        if words.is_empty() {
            return None;
        }

        let command_part = words[0];
        let mut fallthrough_command = None;
        let mut functor = None;

        let commands = self.commands.lock().await;

        for command in commands.iter() {
            if command.prefix.is_empty() {
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
        } else if let Some(fallthrough_command) = fallthrough_command {
            debug!("Going to call fallthrough command \"\".");
            Some(fallthrough_command)
        } else {
            debug!("Didn't find command >{}<, ignoring.", command);
            None
        };

        if let Some(functor) = functor_to_call {
            match functor {
                FunctorType::Basic(functor) => {
                    Some((functor)(command_event, self.state.clone()).await)
                }
                FunctorType::Extra(functor) => {
                    Some((functor)(command_event, self.state.clone(), bot_info).await)
                }
            }
        } else {
            None
        }
    }
}

pub(super) async fn help_command<State>(
    event: nostr::Event,
    _state: State,
    bot_info: BotInfo,
) -> nostr::EventNonSigned {
    nostr::get_reply(event, bot_info.help)
}

type NostrMessageReceiver = tokio::sync::mpsc::Receiver<nostr::Message>;
type NostrMessageSender = tokio::sync::mpsc::Sender<nostr::Message>;

pub(super) struct Profile {
    pub(super) name: Option<String>,
    pub(super) about: Option<String>,
    pub(super) picture_url: Option<String>,
    pub(super) intro_message: Option<String>,
}

impl Profile {
    pub(super) fn new() -> Self {
        Self {
            name: None,
            about: None,
            picture_url: None,
            intro_message: None,
        }
    }
}

pub(super) async fn set_profile(
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
    let message = nostr::get_profile_event(name, about, picture_url).sign(keypair);

    sender.lock().await.send(message).await;
}

pub(super) async fn request_subscription(keypair: &secp256k1::KeyPair, sink: network::Sink) {
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
                return;
            }
        };

        if let tungstenite::Message::Ping(payload) = data.clone() {
            debug!("Received ping from {}, returning pong", stream.peer_addr);
            network::send_message(sink.clone(), tungstenite::Message::Ping(payload));
            return;
        }

        debug!("Got message {:?} from {}.", data, stream.peer_addr);

        let data_str = data.to_string();

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
        network::StreamType::Direct(stream) => {
            let f = stream.for_each(listen);
            f.await;
        }
        network::StreamType::Socks5(stream) => {
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
    proxy_addr: Option<String>,
) {
    info!("Relay listener for {} started.", sink.peer_addr);
    let peer_addr = sink.peer_addr.clone();

    let connection_type = match sink.clone().sink {
        network::SinkType::Direct(_) => network::ConnectionType::Direct,
        network::SinkType::Socks5(_) => network::ConnectionType::Socks5,
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
            let connection =
                network::get_connection(&peer_addr, &connection_type, &proxy_addr).await;
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
