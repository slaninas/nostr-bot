// Simple example of voting bot that reacts to 'yes', 'no' and 'results' commands.
use nostr_bot::{
    format_reply, new_sender, wrap, wrap_extra, Bot, BotInfo, Command, Event, EventNonSigned,
    FunctorType, Network, Sender, State,
};

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

async fn show_relays(event: Event, _state: State<Votes>, bot: BotInfo) -> EventNonSigned {
    let connected_relays = bot.connected_relays().await;
    let text = format!("I'm connected to these relays: {:?}", connected_relays);
    format_reply(event, text)
}

async fn rest(event: Event, _state: State<Votes>) -> EventNonSigned {
    let content = event.content.clone();
    format_reply(
        event,
        format!(
            "You said '{}', that's nice but it's not a command, try !help.",
            content
        ),
    )
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

    let s = String::from("Just a dummy talking.");
    let sender = new_sender();

    let pic_url = "https://thumbs.dreamstime.com/z/poll-survey-results-voting-election-opinion-word-red-d-letters-pie-chart-to-illustrate-opinions-61587174.jpg";
    let mut bot = Bot::<State>::new(keypair, relays, Network::Clearnet)
        .set_name("poll_bot")
        .set_about("Just a bot.")
        .set_picture(pic_url)
        .set_intro_message(&question)
        .command(Command::new("!results", wrap!(results)).desc("Show voting results."))
        .command(Command::new("!yes", wrap!(yes)).desc("Add one 'yes' vote."))
        .command(Command::new("!no", wrap!(no)).desc("Add one 'no' vote."))
        .command(Command::new("!relays", wrap_extra!(show_relays)).desc("Show connected relays."))
        .command(
            Command::new("", wrap!(rest))
                .desc("Special command that is run when no other command matches."),
        )
        .sender(sender.clone())
        .spawn(Box::pin(
            async move { dummy_loop(s, sender, keypair).await },
        ))
        .help();
    bot.run(shared_state).await;
}
