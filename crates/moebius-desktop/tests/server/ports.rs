//! The ports: the app's forwarder carries bytes both ways against a real server, and a port that a shell on the server
//! opens is announced on the event stream and reached through the forwarder.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use iced::futures::channel::mpsc;
use moebius_desktop::client::{
    Client, ForwardEvent, Forwarding, StreamEvent, TerminalEvent, TerminalInput, events,
    serve_forward, terminal,
};
use moebius_desktop::screens::ports::Ports;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::fixtures::{next, start};

/// The next event of the forwarder.
async fn told(events: &mut mpsc::Receiver<ForwardEvent>) -> ForwardEvent {
    tokio::time::timeout(Duration::from_secs(60), events.next())
        .await
        .expect("the forwarder told nothing in a minute")
        .expect("the forwarder ended")
}

/// A forwarder for `remote` on a free local port; returns that port and the channel it tells on.
async fn forwarder(
    settings: &moebius_desktop::settings::Settings,
    remote: u16,
) -> (
    u16,
    mpsc::Receiver<ForwardEvent>,
    tokio::task::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (tx, mut rx) = mpsc::channel(16);
    let task = tokio::spawn(serve_forward(listener, settings.clone(), remote, tx));
    let Forwarding::Listening(local) = told(&mut rx).await.what else {
        panic!("the forwarder does not listen");
    };
    (local, rx, task)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_forwarder_carries_bytes_both_ways_and_says_when_nothing_answers() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start(dir.path()).await;

    // A server on this machine that answers each message with "echo:" and the message.
    let echo = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let remote = echo.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = echo.accept().await.unwrap();
            tokio::spawn(async move {
                let mut buffer = [0u8; 1024];
                while let Ok(n) = stream.read(&mut buffer).await {
                    if n == 0 {
                        break;
                    }
                    let mut answer = b"echo:".to_vec();
                    answer.extend_from_slice(&buffer[..n]);
                    if stream.write_all(&answer).await.is_err() {
                        break;
                    }
                }
            });
        }
    });

    let (local, mut events, task) = forwarder(&settings, remote).await;
    for round in ["first", "second"] {
        let mut connection = TcpStream::connect(("127.0.0.1", local)).await.unwrap();
        connection.write_all(round.as_bytes()).await.unwrap();
        let mut answer = vec![0u8; "echo:".len() + round.len()];
        tokio::time::timeout(Duration::from_secs(60), connection.read_exact(&mut answer))
            .await
            .expect("no answer in a minute")
            .unwrap();
        assert_eq!(String::from_utf8(answer).unwrap(), format!("echo:{round}"));
    }
    task.abort();

    // A port nothing listens on: the connection to the local port ends, and the forwarder says why.
    let unused = {
        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        probe.local_addr().unwrap().port()
    };
    let (local, mut events_of_unused, task) = forwarder(&settings, unused).await;
    let mut connection = TcpStream::connect(("127.0.0.1", local)).await.unwrap();
    let ForwardEvent {
        remote: told_of,
        what: Forwarding::Refused(reason),
    } = told(&mut events_of_unused).await
    else {
        panic!("the connection was not refused");
    };
    assert_eq!(told_of, unused);
    assert!(reason.contains("502"), "{reason}");
    let mut rest = Vec::new();
    connection.read_to_end(&mut rest).await.unwrap();
    assert!(rest.is_empty());
    task.abort();
    events.close();
}

/// A port the shell of the Terminal page opens (by a program that prints it) is announced, listed with the terminal it
/// belongs to, and reached through the forwarder.
#[tokio::test(flavor = "multi_thread")]
async fn a_port_a_shell_opens_is_announced_listed_and_reached_through_the_forwarder() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start(dir.path()).await;
    let mut stream = Box::pin(events(settings.clone()));
    while !matches!(next(&mut stream).await, StreamEvent::Connected) {}

    // The shell runs a Python program (Unix, like the shell itself) that listens on a port of its own and greets.
    let mut shell = Box::pin(terminal(settings.clone(), "/v1/terminal".to_string()));
    let input = loop {
        match shell.next().await.expect("the shell's stream ended") {
            TerminalEvent::Connected(input) => break input,
            TerminalEvent::Closed(reason) => panic!("{reason}"),
            TerminalEvent::Output(_) => {}
        }
    };
    let program = "import socket;s=socket.socket();s.bind(('127.0.0.1',0));s.listen();\
                   c,_=s.accept();c.sendall(b'hello from the shell');c.recv(10)";
    let mut input = input;
    input
        .send(TerminalInput::Bytes(
            format!("python3 -c \"{program}\"\n").into_bytes(),
        ))
        .await
        .unwrap();
    // The shell's stream is read in the background so that the shell stays open.
    let keeping = tokio::spawn(async move { while shell.next().await.is_some() {} });

    let mut ports = Ports::default();
    let opened = loop {
        if let StreamEvent::Event(event) = next(&mut stream).await
            && event.kind == "port_opened"
        {
            break event;
        }
    };
    assert_eq!(opened.data["kind"], "terminal");
    assert_eq!(opened.data["run"], serde_json::Value::Null);
    ports.on_event(&opened.kind, &opened.data, |_| true);
    let port = opened.data["port"].as_u64().unwrap() as u16;
    assert_eq!(ports.detected().len(), 1);
    assert!(
        ports.address(port).is_some(),
        "the port is forwarded at once"
    );
    assert_eq!(ports.offer().map(|offer| offer.port), Some(port));

    // The server lists it too.
    let listed = Client::new(settings.clone()).ports().await.unwrap();
    assert!(
        listed
            .iter()
            .any(|p| p.port == port && p.kind == "terminal")
    );

    // Through the forwarder, the program's greeting arrives.
    let (local, _events, task) = forwarder(&settings, port).await;
    let mut connection = TcpStream::connect(("127.0.0.1", local)).await.unwrap();
    let mut greeting = vec![0u8; "hello from the shell".len()];
    tokio::time::timeout(
        Duration::from_secs(60),
        connection.read_exact(&mut greeting),
    )
    .await
    .expect("no greeting in a minute")
    .unwrap();
    assert_eq!(greeting, b"hello from the shell");
    task.abort();

    // Closing the shell closes the port, and the stream says so.
    drop(connection);
    keeping.abort();
    let closed = loop {
        if let StreamEvent::Event(event) = next(&mut stream).await
            && event.kind == "port_closed"
        {
            break event;
        }
    };
    assert_eq!(closed.data["port"], port);
    ports.on_event(&closed.kind, &closed.data, |_| true);
    assert!(ports.detected().is_empty());
    assert_eq!(ports.address(port), None);
}
