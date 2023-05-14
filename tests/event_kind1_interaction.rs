use nostr_bot::*;
use std::path::Path;
use std::sync::mpsc as syncmpsc;
use std::sync::mpsc::{Receiver as MpscReceiver, Sender as MpscSender};

use tungstenite::{
    connect, handshake::client::Response, stream::MaybeTlsStream, Message, WebSocket,
};
use url::Url;

use std::net::{TcpListener, TcpStream};

// TODO:
// 1) R

#[tokio::test]
async fn kind1_test() {
    let tmp_dir = tempdir::TempDir::new("kind1_test").unwrap();

    let relay_port = get_available_port().unwrap();
    let shutdown_tx = start_server(relay_port, tmp_dir.path());

    let relay = format!("ws://localhost:{relay_port}");

    let keypair = keypair_from_secret(
        // npub1pntawch7y2tus5f36kq8a4yrq404k8kzy5z4wzutgsag9ctkyf8su7re4f
        // 0cd7d762fe2297c85131d5807ed483055f5b1ec22505570b8b443a82e176224f
        "nsec1l26y6vjt20jclcpn689gy07s48matyv4zzc8frml9xhsdjsvp92qzv2ys0",
    );

    let question = String::from("This is testing welcome message.");

    // Wrap your object into Arc<Mutex> so it can be shared among command handlers
    let shared_state = wrap_state(State {});

    let mut bot = Bot::new(keypair, vec![relay.as_str()], shared_state)
        .name("test_bot")
        .about("Just a bot testing itself.")
        .picture("https://i.imgur.com/ij4XprK.jpeg")
        .intro_message(&question)
        .help();

    // Connect to the relay and subscribe to every event
    let connection = try_connect(&relay);
    assert!(!connection.is_err());
    let (mut socket, response) = connection.unwrap();

    let subscription_id = "testing subscription id";
    let subscribe_message = format!(r##"["REQ", "{subscription_id}", {{"since": 0}} ]"##,);

    socket
        .write_message(Message::Text(subscribe_message))
        .unwrap();

    let msg = socket.read_message().expect("Error reading message").into_text().unwrap();
    assert_eq!(msg, format!(r#"["EOSE","{subscription_id}"]"#));

    tokio::spawn(async move {bot.run()});
    // bot.run().await;

    let shutdown_server = shutdown_tx.send(());
    assert!(!shutdown_server.is_err());
}

struct State {}

fn start_server(port: u16, tmp_dir: &Path) -> MpscSender<()> {
    let (ctrl_tx, ctrl_rx): (MpscSender<()>, MpscReceiver<()>) = syncmpsc::channel();

    let mut settings = nostr_rs_relay::config::Settings::default();
    settings.network.port = port;

    settings.database.data_directory = tmp_dir.display().to_string();

    tokio::task::spawn_blocking(move || {
        let _svr = nostr_rs_relay::server::start_server(&settings, ctrl_rx);
    });

    ctrl_tx
}

// fn try_connect(relay: &str) -> (WebSocket<Stream>, Response) {
fn try_connect(relay: &str) -> Result<(WebSocket<MaybeTlsStream<TcpStream>>, Response), ()> {
    for i in 0..5 {
        match connect(Url::parse(&relay).unwrap()) {
            Ok(connection) => return Ok(connection),
            Err(_) => std::thread::sleep(std::time::Duration::from_secs(i + 1)),
        }
    }

    Err(())
}

// From https://github.com/rust-lang-nursery/rust-cookbook/issues/500#issue-384082035
fn port_is_available(port: u16) -> bool {
    match TcpListener::bind(("127.0.0.1", port)) {
        Ok(_) => true,
        Err(_) => false,
    }
}

fn get_available_port() -> Option<u16> {
    (10_000..65_535).find(|port| port_is_available(*port))
}
