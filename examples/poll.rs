use log::warn;
use std::io::Write;

struct Votes {
    question: String,
    yes: u64,
    no: u64,
    filename: String,
}

impl Votes {
    fn from_file(question: String, filename: String) -> Self {
        match std::fs::read_to_string(&filename) {
            Ok(s) => {
                let s = s.split(":").collect::<Vec<_>>();

                Self {
                    question,
                    yes: s[0].parse::<u64>().unwrap(),
                    no: s[1].parse::<u64>().unwrap(),
                    filename,
                }
            }

            Err(e) => {
                warn!(
                    "Unable to open file {}: {}. Using zero values.",
                    filename, e
                );
                Self {
                    question,
                    yes: 0,
                    no: 0,
                    filename,
                }
            }
        }
    }

    fn add_yes(&mut self) {
        self.yes += 1;
        self.update_file();
    }

    fn add_no(&mut self) {
        self.no += 1;
        self.update_file();
    }

    fn update_file(&self) {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(&self.filename)
            .unwrap();
        write!(file, "{}:{}", self.yes, self.no).unwrap();
    }
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

async fn yes(
    event: nostr_bot::nostr::Event,
    state: nostr_bot::State<Votes>,
) -> nostr_bot::nostr::EventNonSigned {
    let mut votes = state.lock().unwrap();
    votes.add_yes();
    nostr_bot::nostr::format_reply(event, format_results(&votes.question, &votes))
}

async fn no(
    event: nostr_bot::nostr::Event,
    state: nostr_bot::State<Votes>,
) -> nostr_bot::nostr::EventNonSigned {
    let mut votes = state.lock().unwrap();
    votes.add_no();
    nostr_bot::nostr::format_reply(event, format_results(&votes.question, &votes))
}

async fn results(
    event: nostr_bot::nostr::Event,
    state: nostr_bot::State<Votes>,
) -> nostr_bot::nostr::EventNonSigned {
    let votes = state.lock().unwrap();
    nostr_bot::nostr::format_reply(event, format_results(&votes.question, &votes))
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

    let question = "Do you think Pluto should be a planet?".to_string();

    type State = nostr_bot::State<Votes>;
    let shared_state =
        nostr_bot::wrap_state(Votes::from_file(question.clone(), "votes".to_string()));

    let pic_url = "https://thumbs.dreamstime.com/z/poll-survey-results-voting-election-opinion-word-red-d-letters-pie-chart-to-illustrate-opinions-61587174.jpg";
    let mut bot =
        nostr_bot::Bot::<State>::new(keypair, relays, nostr_bot::network::Network::Clearnet)
            .set_name("poll_bot")
            .set_about("Just a bot.")
            .set_picture(pic_url)
            .set_intro_message(&question)
            .add_command("results", nostr_bot::wrap!(results))
            .add_command("yes", nostr_bot::wrap!(yes))
            .add_command("no", nostr_bot::wrap!(no));
    bot.run(shared_state).await;
}
