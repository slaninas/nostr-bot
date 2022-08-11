// Example of a voting bot that reacts to '!yes', '!no' and '!results' commands.
use nostr_bot::*;

struct Votes {
    question: String,
    yes: u64,
    no: u64,
}

fn format_results(question: &str, votes: &Votes) -> String {
    format!(
        "{}\n------------------\nyes: {}\nno:  {}",
        question, votes.yes, votes.no
    )
}

async fn yes(event: Event, state: State<Votes>) -> EventNonSigned {
    let mut votes = state.lock().await;
    votes.yes += 1;
    format_reply(event, format_results(&votes.question, &votes))
}

async fn no(event: Event, state: State<Votes>) -> EventNonSigned {
    let mut votes = state.lock().await;
    votes.no += 1;
    format_reply(event, format_results(&votes.question, &votes))
}

async fn results(event: Event, state: State<Votes>) -> EventNonSigned {
    let votes = state.lock().await;
    format_reply(event, format_results(&votes.question, &votes))
}

#[tokio::main]
async fn main() {
    init_logger();

    let relays = vec![
        url::Url::parse("wss://nostr-pub.wellorder.net").unwrap(),
        url::Url::parse("wss://relay.damus.io").unwrap(),
        url::Url::parse("wss://relay.nostr.info").unwrap(),
    ];

    let secp = secp256k1::Secp256k1::new();
    let keypair = secp256k1::KeyPair::from_seckey_str(
        &secp,
        "0000000000000000000000000000000000000000000000000000000000000001", // Your secret goes here
    )
    .unwrap();

    let question = String::from("Do you think Pluto should be a planet?");

    let shared_state = nostr_bot::wrap_state(Votes {
        question: question.clone(),
        yes: 0,
        no: 0,
    });

    Bot::new(keypair, relays, shared_state)
        .name("poll_bot")
        .about("Just a bot.")
        .picture( "https://thumbs.dreamstime.com/z/poll-survey-results-voting-election-opinion-word-red-d-letters-pie-chart-to-illustrate-opinions-61587174.jpg")
        .intro_message(&question)
        .command(Command::new("!yes", wrap!(yes)))
        .command(Command::new("!no", wrap!(no)))
        .command(Command::new("!results", wrap!(results)))
        .run().await;
}
