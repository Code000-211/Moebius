//! Compression of the desktop API's answers: JSON and the web app's files of 1 KiB or more are answered with gzip or
//! brotli when the request accepts it, and a stream (the event WebSocket) is left alone.

use std::io::Read;
use std::sync::Arc;

use moebius_core::config::Config;
use moebius_core::server::Server;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

struct Fixture {
    _dir: tempfile::TempDir,
    api: std::net::SocketAddr,
    token: String,
}

async fn start() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("moebius.toml");
    std::fs::write(&file, "[router]\nlisten = \"127.0.0.1:0\"\n").unwrap();
    let config = Config::load(&file).unwrap();
    let api = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let router = tokio::net::TcpListener::bind(config.router_listen)
        .await
        .unwrap();
    let (api_addr, router_addr) = (api.local_addr().unwrap(), router.local_addr().unwrap());
    let server = Arc::new(Server::new(config, router_addr).unwrap());
    let token = server.token.to_string();
    tokio::spawn(server.serve(api, router));
    Fixture {
        _dir: dir,
        api: api_addr,
        token,
    }
}

/// A client that neither asks for compression nor undoes it, so the test sees what is on the wire.
fn client() -> reqwest::Client {
    reqwest::Client::builder().build().unwrap()
}

#[tokio::test]
async fn a_large_json_answer_is_gzipped_when_the_request_accepts_it_and_not_otherwise() {
    let f = start().await;
    let url = format!("http://{}/v1/settings", f.api);
    let plain = client()
        .get(&url)
        .bearer_auth(&f.token)
        .send()
        .await
        .unwrap();
    assert!(plain.headers().get("content-encoding").is_none());
    let plain = plain.bytes().await.unwrap();
    assert!(
        plain.len() > 1024,
        "{} bytes is too small to test",
        plain.len()
    );

    let packed = client()
        .get(&url)
        .bearer_auth(&f.token)
        .header("accept-encoding", "gzip")
        .send()
        .await
        .unwrap();
    assert_eq!(packed.headers()["content-encoding"], "gzip");
    assert!(
        packed.headers()["vary"]
            .to_str()
            .unwrap()
            .to_ascii_lowercase()
            .contains("accept-encoding")
    );
    let packed = packed.bytes().await.unwrap();
    assert!(packed.len() < plain.len() / 2);
    let mut unpacked = Vec::new();
    flate2::read::GzDecoder::new(&packed[..])
        .read_to_end(&mut unpacked)
        .unwrap();
    assert_eq!(unpacked, plain.to_vec());

    let brotli = client()
        .get(&url)
        .bearer_auth(&f.token)
        .header("accept-encoding", "br")
        .send()
        .await
        .unwrap();
    assert_eq!(brotli.headers()["content-encoding"], "br");
}

#[tokio::test]
async fn a_small_answer_is_not_compressed() {
    let f = start().await;
    let answer = client()
        .get(format!("http://{}/v1/health", f.api))
        .bearer_auth(&f.token)
        .header("accept-encoding", "gzip, br")
        .send()
        .await
        .unwrap();
    assert!(answer.headers().get("content-encoding").is_none());
}

#[tokio::test]
async fn the_event_stream_still_opens_and_delivers_when_the_client_accepts_compression() {
    let f = start().await;
    let mut request = format!("ws://{}/v1/events", f.api)
        .into_client_request()
        .unwrap();
    request.headers_mut().insert(
        "authorization",
        format!("Bearer {}", f.token).parse().unwrap(),
    );
    request
        .headers_mut()
        .insert("accept-encoding", "gzip, br".parse().unwrap());
    let (mut socket, response) = tokio_tungstenite::connect_async(request).await.unwrap();
    assert!(response.headers().get("content-encoding").is_none());
    // Something happens that the stream reports: a ping answers with a pong on the same socket.
    use futures_util::{SinkExt, StreamExt};
    socket.send(Message::Ping(vec![1].into())).await.unwrap();
    loop {
        match socket.next().await.unwrap().unwrap() {
            Message::Pong(_) => break,
            _ => continue,
        }
    }
}
