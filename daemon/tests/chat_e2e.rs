//! End-to-end tests for `--chat`: two Iroh endpoints on 127.0.0.1 (no relays, no
//! internet) chat through `chat::converse`.

use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use sovereign_daemon::chat::{self, MAX_LINE};
use sovereign_daemon::p2p::{self, Accept, P2p, SecretKey};

/// A waiting side that accepts chats, and a caller that knows its local address.
async fn pair() -> (P2p, P2p) {
    let waiter = P2p::bind_local(SecretKey::generate(), Accept::Chat)
        .await
        .unwrap();
    let caller = P2p::bind_local(SecretKey::generate(), Accept::Nothing)
        .await
        .unwrap();
    caller.add_peer_addr(waiter.id(), waiter.local_addrs()[0]);
    (waiter, caller)
}

#[tokio::test]
async fn messages_arrive_and_hang_up_is_noticed() {
    let (waiter, caller) = pair().await;
    let waiter_id = waiter.id();
    let caller_id = caller.id();

    // The caller types two lines (one with a terminal colour trick), then its input
    // ends, like pressing Ctrl+D. It should still see the waiter's reply.
    let caller_task = tokio::spawn(async move {
        let tunnel = caller.open_chat(waiter_id).await.unwrap();
        let typed: &[u8] = b"hello there\n\x1b[31mnot red\n";
        let mut input = BufReader::new(typed).lines();
        let mut output = Vec::new();
        chat::converse(tunnel, &mut input, &mut output)
            .await
            .unwrap();
        String::from_utf8(output).unwrap()
    });

    let tunnel = waiter
        .next_incoming()
        .await
        .unwrap()
        .accept()
        .await
        .unwrap();
    assert_eq!(tunnel.peer(), caller_id);
    let typed: &[u8] = b"hi back\n";
    let mut input = BufReader::new(typed).lines();
    let mut output = Vec::new();
    tokio::time::timeout(
        Duration::from_secs(10),
        chat::converse(tunnel, &mut input, &mut output),
    )
    .await
    .unwrap()
    .unwrap();

    let caller_name = p2p::short_id(&caller_id);
    let waiter_name = p2p::short_id(&waiter_id);
    let shown = String::from_utf8(output).unwrap();
    assert!(
        shown.contains(&format!("{caller_name}: hello there\n")),
        "got: {shown}"
    );
    assert!(
        shown.contains(&format!("{caller_name}: \u{FFFD}[31mnot red\n")),
        "got: {shown}"
    );
    assert!(
        !shown.contains('\x1b'),
        "escape codes must not reach the terminal"
    );
    assert!(
        shown.contains(&format!("{caller_name} has left.")),
        "got: {shown}"
    );

    let caller_saw = tokio::time::timeout(Duration::from_secs(10), caller_task)
        .await
        .unwrap()
        .unwrap();
    assert!(
        caller_saw.contains(&format!("{waiter_name}: hi back\n")),
        "got: {caller_saw}"
    );
}

#[tokio::test]
async fn too_long_message_hangs_up() {
    let (waiter, caller) = pair().await;
    let waiter_id = waiter.id();

    let caller_task = tokio::spawn(async move {
        let mut tunnel = caller.open_chat(waiter_id).await.unwrap();
        // One endless line with no newline.
        let _ = tunnel.write_all(&vec![b'x'; MAX_LINE * 2]).await;
        tunnel.closed().await;
    });

    let tunnel = waiter
        .next_incoming()
        .await
        .unwrap()
        .accept()
        .await
        .unwrap();
    let (_keep_open, input) = tokio::io::duplex(64);
    let mut input = BufReader::new(input).lines();
    let mut output = Vec::new();
    tokio::time::timeout(
        Duration::from_secs(10),
        chat::converse(tunnel, &mut input, &mut output),
    )
    .await
    .unwrap()
    .unwrap();

    let shown = String::from_utf8(output).unwrap();
    assert!(shown.contains("too long; hanging up"), "got: {shown}");
    assert!(
        !shown.contains("xxxx"),
        "the long message must not be shown"
    );
    caller_task.await.unwrap();
}
