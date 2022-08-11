Do you want to run your own nostr bot? You've come to the right place.
This crate makes it easy for you to implement bot that reacts to [nostr](https://github.com/nostr-protocol/nostr) events.

## Usage
This crate is on [crates.io](https://crates.io/crates/nostr-bot) and can be
used by adding `nostr-bot` to your dependencies in your project's `Cargo.toml`.

Be aware that this crate is still being developed and until 1.0 is out there may be API breaking changes
even in MINOR (see [SemVer](https://semver.org/)) releases, PATCHES should be compatible
so If you want highly inscrease changes that the API stays compatible with your code commit to a specific MAJOR.MINOR version:

```toml
[dependencies]
nostr-bot = "0.1"

```
Check [Bot] to see main struct of the crate.

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
// eventwhich is then signed using the bot's key and send to relays
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

    // And now the Bot
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
