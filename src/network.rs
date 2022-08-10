use futures_util::sink::SinkExt;
use futures_util::StreamExt;
use log::{debug, info, warn};
use tokio_socks::tcp::Socks5Stream;

use futures_util::stream::{SplitSink, SplitStream};
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;

type WebSocket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
type WebSocketTor = tokio_tungstenite::WebSocketStream<Socks5Stream<tokio::net::TcpStream>>;

type SplitSinkClearnet = futures_util::stream::SplitSink<
    WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tungstenite::Message,
>;
type StreamClearnet = futures_util::stream::SplitStream<
    WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

type SplitSinkTor = SplitSink<
    WebSocketStream<Socks5Stream<tokio::net::TcpStream>>,
    tokio_tungstenite::tungstenite::Message,
>;
type StreamTor = SplitStream<WebSocketStream<Socks5Stream<tokio::net::TcpStream>>>;

#[derive(Clone, Debug)]
pub enum SinkType {
    Clearnet(std::sync::Arc<tokio::sync::Mutex<SplitSinkClearnet>>),
    Tor(std::sync::Arc<tokio::sync::Mutex<SplitSinkTor>>),
}

#[derive(Debug)]
pub enum StreamType {
    Clearnet(StreamClearnet),
    Tor(StreamTor),
}

#[derive(Clone)]
pub struct Sink {
    pub sink: SinkType,
    pub peer_addr: url::Url,
}

impl Sink {
    pub async fn update(&mut self, new_sink: SinkType) {
        match new_sink {
            SinkType::Clearnet(new_arc) => match &self.sink {
                SinkType::Clearnet(old_arc) => {
                    let mut x = old_arc.lock().await;
                    let a = std::sync::Arc::try_unwrap(new_arc).unwrap().into_inner();
                    debug!("Updated sink");
                    *x = a;
                }
                SinkType::Tor(_) => {
                    panic!("Trying to assing clearnet sink to tor sink.")
                }
            },
            SinkType::Tor(new_arc) => match &self.sink {
                SinkType::Clearnet(_) => {
                    panic!("Trying to assing tor sink to clearnet sink.")
                }
                SinkType::Tor(old_arc) => {
                    let mut x = old_arc.lock().await;
                    let a = std::sync::Arc::try_unwrap(new_arc).unwrap().into_inner();
                    *x = a;
                    debug!("Updated sink");
                }
            },
        }
    }
}

pub struct Stream {
    pub stream: StreamType,
    pub peer_addr: url::Url,
}

pub enum Network {
    Clearnet,
    Tor,
}

pub async fn send_to_all(msg: String, sinks: Vec<Sink>) {
    for sink in sinks {
        send(msg.clone(), sink).await;
    }
}

pub async fn send(msg: String, sink_wrap: Sink) {
    let result = match sink_wrap.sink {
        SinkType::Clearnet(sink) => {
            debug!("Sending >{}< to {} over clearnet", msg, sink_wrap.peer_addr);
            sink.lock()
                .await
                .send(tungstenite::Message::Text(msg))
                .await
        }
        SinkType::Tor(sink) => {
            debug!("Sending >{}< to {} over tor", msg, sink_wrap.peer_addr);
            sink.lock()
                .await
                .send(tungstenite::Message::Text(msg))
                .await
        }
    };

    match result {
        Ok(_) => {}
        // relay_listener is handling the connection and warns when the connection is lost so debug
        // is sufficient here, no need to use warn!
        Err(e) => debug!("Unable to send message to {}: {}", sink_wrap.peer_addr, e),
    }
}

pub async fn ping(sink_wrap: Sink) -> bool {
    let msg = "ping".to_string();
    let result = match sink_wrap.sink {
        SinkType::Clearnet(sink) => {
            debug!("Sending >{}< to {} over clearnet", msg, sink_wrap.peer_addr);
            sink.lock()
                .await
                .send(tungstenite::Message::Text(msg))
                .await
        }
        SinkType::Tor(sink) => {
            debug!("Sending >{}< to {} over tor", msg, sink_wrap.peer_addr);
            sink.lock()
                .await
                .send(tungstenite::Message::Text(msg))
                .await
        }
    };

    match result {
        Ok(_) => true,
        // relay_listener is handling the connection and warns when the connection is lost so debug
        // is sufficient here, no need to use warn!
        Err(e) => {
            debug!("Unable to send message to {}: {}", sink_wrap.peer_addr, e);
            false
        }
    }
}

pub async fn try_connect(relays: &Vec<url::Url>, network: &Network) -> (Vec<Sink>, Vec<Stream>) {
    let mut sinks = vec![];
    let mut streams = vec![];

    for relay in relays {
        let connection = get_connection(relay, network).await;

        if let Ok((sink, stream)) = connection {
            sinks.push(sink);
            streams.push(stream);
        }
    }

    (sinks, streams)
}

pub async fn get_connection(relay: &url::Url, network: &Network) -> Result<(Sink, Stream), String> {
    match network {
        Network::Tor => {
            let ws_stream = connect_proxy(relay).await;
            match ws_stream {
                Ok(ws_stream) => {
                    let (sink, stream) = ws_stream.split();
                    let sink = Sink {
                        sink: SinkType::Tor(std::sync::Arc::new(tokio::sync::Mutex::new(sink))),
                        peer_addr: relay.clone(),
                    };
                    let stream = Stream {
                        stream: StreamType::Tor(stream),
                        peer_addr: relay.clone(),
                    };
                    Ok((sink, stream))
                }
                Err(e) => {
                    warn!("Unable to connect to {}", relay);
                    Err(e.to_string())
                }
            }
        }

        Network::Clearnet => {
            let ws_stream = connect(relay).await;
            match ws_stream {
                Ok(ws_stream) => {
                    let (sink, stream) = ws_stream.split();
                    let sink = Sink {
                        sink: SinkType::Clearnet(std::sync::Arc::new(tokio::sync::Mutex::new(
                            sink,
                        ))),
                        peer_addr: relay.clone(),
                    };
                    let stream = Stream {
                        stream: StreamType::Clearnet(stream),
                        peer_addr: relay.clone(),
                    };
                    Ok((sink, stream))
                }
                Err(e) => {
                    warn!("Unable to connect to {}", relay);
                    Err(e.to_string())
                }
            }
        }
    }
}

async fn connect(relay: &url::Url) -> Result<WebSocket, tungstenite::Error> {
    info!("Connecting to {} using clearnet", relay);
    let (ws_stream, _response) = tokio_tungstenite::connect_async(relay).await?;
    info!("Connected to {}", relay);
    Ok(ws_stream)
}

const TCP_PROXY_ADDR: &str = "127.0.0.1:9050";

async fn connect_proxy(relay: &url::Url) -> Result<WebSocketTor, tungstenite::Error> {
    info!("Connecting to {} using tor", relay);
    let ws_onion_addr = relay;

    let host = match ws_onion_addr.host_str() {
        Some(addr) => addr,
        None => panic!("Unable to parse >{}<", ws_onion_addr),
    };

    let port = match ws_onion_addr.port() {
        Some(port) => port.to_string(),
        None => "".to_string(),
    };

    let onion_addr = format!("{}:{}", host, port);
    debug!("onion_addr >{}:{}<", host, port);

    let socket = TcpStream::connect(TCP_PROXY_ADDR).await.unwrap();
    socket.set_nodelay(true).unwrap();
    let conn = Socks5Stream::connect_with_socket(socket, onion_addr)
        .await
        .unwrap();

    let (ws_stream, _response) = tokio_tungstenite::client_async(ws_onion_addr, conn).await?;
    info!("Connected to {}", relay);
    Ok(ws_stream)
}
