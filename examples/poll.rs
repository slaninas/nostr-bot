use log::{debug, info};

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

fn bla<State>(event: nostr_bot::nostr::Event, _state: State) -> nostr_bot::nostr::EventNonSigned {
    let msg = event.content.clone();
    nostr_bot::nostr::format_reply(event, format!("Congrats for saying {}", msg))
}

#[tokio::main]
async fn main() {
    let _start = std::time::Instant::now();
    env_logger::Builder::from_default_env()
        // .format(move |buf, rec| {
        // let t = start.elapsed().as_secs_f32();
        // writeln!(buf, "{:.03} [{}] - {}", t, rec.level(), rec.args())
        // })
        .init();

    let network = nostr_bot::network::Network::Clearnet;

    let config_path = std::path::PathBuf::from("config");
    let config = nostr_bot::utils::parse_config(&config_path);

    debug!("{:?}", config);

    let secp = secp256k1::Secp256k1::new();
    let keypair = secp256k1::KeyPair::from_seckey_str(&secp, &config.secret).unwrap();

    type State = nostr_bot::State<Votes>;
    let state = std::sync::Arc::new(std::sync::Mutex::new(Votes {
        question: "Do you think Pluto should be a planet?".to_string(),
        yes: 0,
        no: 0,
    }));

    let c_results: nostr_bot::Closure<State> =
        Box::new(move |event: nostr_bot::nostr::Event, state: State| {
            let s = state.lock().unwrap();
            nostr_bot::nostr::format_reply(event, format_results(&s.question, &s))
        });

    let c_yes: nostr_bot::Closure<State> =
        Box::new(move |event: nostr_bot::nostr::Event, state: State| {
            let mut s = state.lock().unwrap();
            s.yes += 1;
            nostr_bot::nostr::format_reply(event, format_results(&s.question, &s))
        });

    let c_no: nostr_bot::Closure<State> =
        Box::new(move |event: nostr_bot::nostr::Event, state: State| {
            let mut s = state.lock().unwrap();
            s.no += 1;
            nostr_bot::nostr::format_reply(event, format_results(&s.question, &s))
        });

    let pic_url = "https://thumbs.dreamstime.com/z/poll-survey-results-voting-election-opinion-word-red-d-letters-pie-chart-to-illustrate-opinions-61587174.jpg";
    let mut bot = nostr_bot::Bot::<State>::new(keypair, config.relays, network)
        .set_name("blabla")
        .set_about("Boooooooooooooooooooooot")
        .set_picture(pic_url)
        .set_intro_message("Wasup bro")
        .add_command("results", c_results)
        .add_command("yes", c_yes)
        .add_command("no", c_no)
        .add_command("", Box::new(bla));

    // let commands = get_commands::<(u64,u64)>(state.clone());
    info!("Starting bot");
    bot.run(state).await;
}
