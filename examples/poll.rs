// Simple example of voting bot that reacts to 'yes', 'no' and 'results' commands.
use nostr_bot::{format_reply, wrap, Event, EventNonSigned, Network, State};

struct Votes {
    question: String,
    yes: u64,
    no: u64,
}

fn format_results(question: &str, votes: &Votes) -> String {
    let yes = votes.yes as f32;
    let no = votes.no as f32;
    let (yes_perc, no_perc) = {
        if votes.yes == 0 && votes.no == 0 {
            (0.0, 0.0)
        } else {
            (yes / (yes + no) * 100.0, no / (yes + no) * 100.0)
        }
    };
    format!(
        "{}\\n------------------\\nyes: {:.2} % ({})\\nno:  {:.2} % ({})",
        question, yes_perc, yes, no_perc, no
    )
}

async fn yes(event: Event, state: State<Votes>) -> EventNonSigned {
    let mut votes = state.lock().unwrap();
    votes.yes += 1;
    format_reply(event, format_results(&votes.question, &votes))
}

async fn no(event: Event, state: State<Votes>) -> EventNonSigned {
    let mut votes = state.lock().unwrap();
    votes.no += 1;
    format_reply(event, format_results(&votes.question, &votes))
}

async fn results(event: Event, state: State<Votes>) -> EventNonSigned {
    let votes = state.lock().unwrap();
    format_reply(event, format_results(&votes.question, &votes))
}

#[tokio::main]
async fn main() {
    nostr_bot::init_logger();

    let relays = vec![
        String::from("wss://nostr-pub.wellorder.net"),
        String::from("wss://relay.damus.io"),
        String::from("wss://relay.nostr.info"),
    ];

    let mut secret = std::fs::read_to_string("secret").unwrap();
    secret.pop(); // Remove newline

    let secp = secp256k1::Secp256k1::new();
    let keypair = secp256k1::KeyPair::from_seckey_str(&secp, &secret).unwrap();

    let question = String::from("Do you think Pluto should be a planet?");

    type State = nostr_bot::State<Votes>;
    let shared_state = nostr_bot::wrap_state(Votes {
        question: question.clone(),
        yes: 0,
        no: 0,
    });

    let pic_url = "https://thumbs.dreamstime.com/z/poll-survey-results-voting-election-opinion-word-red-d-letters-pie-chart-to-illustrate-opinions-61587174.jpg";
    let mut bot = nostr_bot::Bot::<State>::new(keypair, relays, Network::Clearnet)
        .set_name("poll_bot")
        .set_about("Just a bot.")
        .set_picture(pic_url)
        .set_intro_message(&question)
        .add_command("results", wrap!(results))
        .add_command("yes", wrap!(yes))
        .add_command("no", wrap!(no));
    bot.run(shared_state).await;
}
