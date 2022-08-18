# Nostr Bot

![workflow](https://github.com/slaninas/nostr-bot/actions/workflows/rust.yml/badge.svg)

Do you want to run your own nostr bot? You've come to the right place.
This crate makes it easy to implement your own bot that reacts to [nostr](https://github.com/nostr-protocol/nostr) events, using [tokio](https://github.com/tokio-rs/tokio).

This crate is young and still being developed so there may be hiccups. If you find any issue, don't hesitate to create PR/issue on [GitHub](https://github.com/slaninas/nostr-bot).

Full documentation is available [here](https://docs.rs/nostr-bot/latest/nostr_bot/).

## Usage

This crate is on [crates.io](https://crates.io/crates/nostr-bot) and can be
used by adding `nostr-bot` to your dependencies in your project's `Cargo.toml`.

```toml
[dependencies]
nostr-bot = "0.2"

```

You can also use GitHub [repository](https://github.com/slaninas/nostr-bot):
```toml
[dependencies]
nostr-bot = { git = "https://github.com/slaninas/nostr-bot", rev = "v0.2.3" }
```

Or you can clone the [repository](https://github.com/slaninas/nostr-bot) and use it locally:
```toml
[dependency]
nostr-bot = { path = "your/path/to/nostr-bot" }
```



## Example
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

// Following functions are command responses, you are getting nostr event
// and shared state as arguments and you are supposed to return non-signed
// event which is then signed using the bot's key and send to relays
async fn yes(event: Event, state: State) -> EventNonSigned {
    let mut votes = state.lock().await;
    votes.yes += 1;

    // Use formatted voting results to create new event
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
        "wss://nostr-pub.wellorder.net",
        "wss://relay.damus.io",
        "wss://relay.nostr.info",
    ];

    let keypair = keypair_from_secret(
        // Your secret goes here
        "0000000000000000000000000000000000000000000000000000000000000001",
    );

    let question = String::from("Do you think Pluto should be a planet?");

    // Wrap your object into Arc<Mutex> so it can be shared among command handlers
    let shared_state = wrap_state(Votes {
        question: question.clone(),
        yes: 0,
        no: 0,
    });

    // And now the Bot
    Bot::new(keypair, relays, shared_state)
        // You don't have to set these but then the bot will have incomplete profile info :(
        .name("poll_bot")
        .about("Just a bot.")
        .picture("https://i.imgur.com/ij4XprK.jpeg")
        .intro_message(&question)
        // You don't have to specify any command but then what will the bot do? Nothing.
        .command(Command::new("!yes", wrap!(yes)))
        .command(Command::new("!no", wrap!(no)))
        .command(Command::new("!results", wrap!(results)))
        // And finally run it
        .run()
        .await;
}
```
