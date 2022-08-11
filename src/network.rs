use futures_util::sink::SinkExt;
use futures_util::StreamExt;
use log::{debug, info, warn};
use tokio_socks::tcp::Socks5Stream;

use futures_util::stream::{SplitSink, SplitStream};
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;

type WebSocket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
type WebSocketSocks5 = tokio_tungstenite::WebSocketStream<Socks5Stream<tokio::net::TcpStream>>;

type SplitSinkDirect = futures_util::stream::SplitSink<
    WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tungstenite::Message,
>;
type StreamClearnet = futures_util::stream::SplitStream<
    WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

type SplitSinkSOCKS5 = SplitSink<
    WebSocketStream<Socks5Stream<tokio::net::TcpStream>>,
    tokio_tungstenite::tungstenite::Message,
>;
type StreamSocks5 = SplitStream<WebSocketStream<Socks5Stream<tokio::net::TcpStream>>>;

#[derive(Clone, Debug)]
pub enum SinkType {
    Direct(std::sync::Arc<tokio::sync::Mutex<SplitSinkDirect>>),
    Socks5(std::sync::Arc<tokio::sync::Mutex<SplitSinkSOCKS5>>),
}

#[derive(Debug)]
pub enum StreamType {
    Direct(StreamClearnet),
    Socks5(StreamSocks5),
}

#[derive(Clone)]
pub struct Sink {
    pub sink: SinkType,
    pub peer_addr: url::Url,
}

impl Sink {
    pub async fn update(&mut self, new_sink: SinkType) {
        match new_sink {
            SinkType::Direct(new_arc) => match &self.sink {
                SinkType::Direct(old_arc) => {
                    let mut x = old_arc.lock().await;
                    let a = std::sync::Arc::try_unwrap(new_arc).unwrap().into_inner();
                    debug!("Updated sink");
                    *x = a;
                }
                SinkType::Socks5(_) => {
                    panic!("Trying to assing direct connection sink to socks5 sink.")
                }
            },
            SinkType::Socks5(new_arc) => match &self.sink {
                SinkType::Direct(_) => {
                    panic!("Trying to assing socks5 sink to direct connection sink.")
                }
                SinkType::Socks5(old_arc) => {
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

/// Direct connection to the internet or would you rather like a socks5 proxy?
pub enum ConnectionType {
    Direct,
    Socks5,
}

pub async fn send_to_all(msg: String, sinks: Vec<Sink>) {
    for sink in sinks {
        send(msg.clone(), sink).await;
    }
}

pub async fn send(msg: String, sink_wrap: Sink) {
    let result = match sink_wrap.sink {
        SinkType::Direct(sink) => {
            debug!(
                "Sending >{}< to {} over direct internet connection.",
                msg, sink_wrap.peer_addr
            );
            sink.lock()
                .await
                .send(tungstenite::Message::Text(msg))
                .await
        }
        SinkType::Socks5(sink) => {
            debug!(
                "Sending >{}< to {} over socks5 connection.",
                msg, sink_wrap.peer_addr
            );
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
        SinkType::Direct(sink) => {
            debug!("Sending >{}< to {} over Direct", msg, sink_wrap.peer_addr);
            sink.lock()
                .await
                .send(tungstenite::Message::Text(msg))
                .await
        }
        SinkType::Socks5(sink) => {
            debug!("Sending >{}< to {} over socks5", msg, sink_wrap.peer_addr);
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

pub async fn try_connect(
    relays: &Vec<url::Url>,
    network: &ConnectionType,
    proxy_addr: &Option<url::Url>,
) -> (Vec<Sink>, Vec<Stream>) {
    let mut sinks = vec![];
    let mut streams = vec![];

    for relay in relays {
        let connection = get_connection(relay, network, proxy_addr).await;

        if let Ok((sink, stream)) = connection {
            sinks.push(sink);
            streams.push(stream);
        }
    }

    (sinks, streams)
}

pub async fn get_connection(
    relay: &url::Url,
    network: &ConnectionType,
    proxy_addr: &Option<url::Url>,
) -> Result<(Sink, Stream), String> {
    match network {
        ConnectionType::Socks5 => {
            let proxy_addr = if let Some(proxy_addr) = proxy_addr {
                proxy_addr
            } else {
                panic!("Proxy address has to be specified for socks5 connections.")
            };

            let ws_stream = connect_proxy(relay, proxy_addr).await;
            match ws_stream {
                Ok(ws_stream) => {
                    let (sink, stream) = ws_stream.split();
                    let sink = Sink {
                        sink: SinkType::Socks5(std::sync::Arc::new(tokio::sync::Mutex::new(sink))),
                        peer_addr: relay.clone(),
                    };
                    let stream = Stream {
                        stream: StreamType::Socks5(stream),
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

        ConnectionType::Direct => {
            let ws_stream = connect(relay).await;
            match ws_stream {
                Ok(ws_stream) => {
                    let (sink, stream) = ws_stream.split();
                    let sink = Sink {
                        sink: SinkType::Direct(std::sync::Arc::new(tokio::sync::Mutex::new(sink))),
                        peer_addr: relay.clone(),
                    };
                    let stream = Stream {
                        stream: StreamType::Direct(stream),
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
    info!("Connecting to {} using direct internet connection.", relay);
    let (ws_stream, _response) = tokio_tungstenite::connect_async(relay).await?;
    info!("Connected to {}", relay);
    Ok(ws_stream)
}

async fn connect_proxy(
    relay: &url::Url,
    proxy_addr: &url::Url,
) -> Result<WebSocketSocks5, tungstenite::Error> {
    info!("Connecting to {} using socks5", relay);
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
    debug!("onion_addr {}:{}", host, port);

    let socks5_addr = format!(
        "{}:{}",
        proxy_addr.host_str().expect("Unable to parse socks5 host."),
        proxy_addr.port().expect("Unable to parse socks5 port.")
    );
    debug!("socks5 addr {}", socks5_addr);

    let socket = TcpStream::connect(socks5_addr.as_str()).await?;
    socket.set_nodelay(true).unwrap();
    let conn = Socks5Stream::connect_with_socket(socket, onion_addr)
        .await
        .unwrap();

    let (ws_stream, _response) = tokio_tungstenite::client_async(ws_onion_addr, conn).await?;
    info!("Connected to {}", relay);
    Ok(ws_stream)
}
