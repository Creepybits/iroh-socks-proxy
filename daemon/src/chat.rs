//! Terminal chat between two `.anon` addresses (`--chat`), with no server in between.
//!
//! In plain terms: one side waits, the other calls its address. Iroh opens an encrypted
//! connection between the two daemons, and each line typed is sent as one line of text
//! over a stream of that connection. Think of two Python programs talking over a
//! socket with `sock.sendall(line.encode() + b"\n")`, except the socket is found by
//! key instead of IP address and is end-to-end encrypted.

use std::io;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, Lines};

use crate::p2p::{self, Accept, Error, P2p, PeerId, SecretKey, Tunnel};

/// Longest message accepted, in bytes. Stops a peer from filling our memory with one
/// endless line.
pub const MAX_LINE: usize = 4096;

/// Runs chat mode: calls `peer` if given, otherwise waits for others to call us.
/// Reads messages from the terminal and prints what the other side sends.
pub async fn run(secret_key: SecretKey, peer: Option<PeerId>) -> Result<(), Error> {
    let accept = if peer.is_some() {
        Accept::Nothing
    } else {
        Accept::Chat
    };
    let p2p = P2p::bind_public(secret_key, accept).await?;
    println!("🔑 Your .anon address: {}", p2p::anon_address(&p2p.id()));

    let mut input = tokio::io::BufReader::new(tokio::io::stdin()).lines();
    let mut output = tokio::io::stdout();
    match peer {
        Some(peer) => {
            println!("📞 Calling {}…", p2p::anon_address(&peer));
            let tunnel = p2p.open_chat(peer).await.map_err(|e| {
                format!(
                    "could not reach that address ({}). Is the other side running `--chat` and online?",
                    e
                )
            })?;
            println!("💬 Connected. Type a message and press Enter. Ctrl+C quits.");
            converse(tunnel, &mut input, &mut output).await?;
        }
        None => {
            println!("👂 Waiting for someone to chat with you. Share the address above.");
            while let Some(incoming) = p2p.next_incoming().await {
                match incoming.accept().await {
                    Ok(tunnel) => {
                        println!("💬 {} joined.", p2p::anon_address(&tunnel.peer()));
                        println!("   Type a message and press Enter. Ctrl+C quits.");
                        converse(tunnel, &mut input, &mut output).await?;
                        println!("👂 Waiting for the next person.");
                    }
                    Err(e) => println!("⚠️  Someone tried to connect but failed: {}", e),
                }
            }
        }
    }
    p2p.close().await;
    Ok(())
}

/// One conversation: each line from `input` goes to the peer, and each line from the
/// peer is written to `output` as `<short id>: <text>`. Returns when the peer leaves.
/// When `input` ends, we stop sending but still show what the peer sends.
pub async fn converse<I, O>(
    mut tunnel: Tunnel,
    input: &mut Lines<I>,
    output: &mut O,
) -> io::Result<()>
where
    I: AsyncBufRead + Unpin,
    O: AsyncWrite + Unpin,
{
    let result = talk(&mut tunnel, input, output).await;
    // Make sure our last messages arrive before the connection closes.
    tunnel.finish().await;
    result
}

async fn talk<I, O>(tunnel: &mut Tunnel, input: &mut Lines<I>, output: &mut O) -> io::Result<()>
where
    I: AsyncBufRead + Unpin,
    O: AsyncWrite + Unpin,
{
    let name = p2p::short_id(&tunnel.peer());
    let mut pending: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 1024];
    let mut input_done = false;

    loop {
        tokio::select! {
            // `next_line` and `read` are both safe to cancel, so no text is lost
            // when the other branch wins.
            line = input.next_line(), if !input_done => match line? {
                Some(line) if line.len() > MAX_LINE => {
                    say(output, &format!("(not sent: longer than {} bytes)", MAX_LINE)).await?;
                }
                Some(line) => {
                    let mut message = line.into_bytes();
                    message.push(b'\n');
                    if tunnel.write_all(&message).await.is_err() {
                        say(output, &format!("{} has left.", name)).await?;
                        return Ok(());
                    }
                }
                None => {
                    input_done = true;
                    let _ = tunnel.shutdown().await;
                }
            },
            read = tunnel.read(&mut chunk) => {
                let n = match read {
                    Ok(0) | Err(_) => {
                        say(output, &format!("{} has left.", name)).await?;
                        return Ok(());
                    }
                    Ok(n) => n,
                };
                pending.extend_from_slice(&chunk[..n]);
                while let Some(end) = pending.iter().position(|&b| b == b'\n') {
                    let line: Vec<u8> = pending.drain(..=end).collect();
                    say(output, &format!("{}: {}", name, clean(&line))).await?;
                }
                if pending.len() > MAX_LINE {
                    say(output, &format!("{} sent a message that is too long; hanging up.", name)).await?;
                    return Ok(());
                }
            }
        }
    }
}

async fn say<O: AsyncWrite + Unpin>(output: &mut O, text: &str) -> io::Result<()> {
    output.write_all(format!("{}\n", text).as_bytes()).await?;
    output.flush().await
}

/// Turns a received line into safe text for the terminal. Control characters (which
/// could move the cursor, change colours or fake other output) and text-direction
/// overrides (which could make text read differently than it is) become `�`.
pub fn clean(line: &[u8]) -> String {
    let text = String::from_utf8_lossy(line);
    let text = text.strip_suffix('\n').unwrap_or(&text);
    let text = text.strip_suffix('\r').unwrap_or(text);
    text.chars()
        .map(|c| match c {
            '\t' => ' ',
            c if c.is_control() => '\u{FFFD}',
            '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' => '\u{FFFD}',
            c => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_keeps_normal_text() {
        assert_eq!(clean("hej på dig 👋\r\n".as_bytes()), "hej på dig 👋");
        assert_eq!(clean(b"a\tb\n"), "a b");
    }

    #[test]
    fn clean_defuses_terminal_tricks() {
        assert_eq!(clean(b"\x1b[31mred\n"), "\u{FFFD}[31mred");
        assert_eq!(clean(b"over\rwrite"), "over\u{FFFD}write");
        assert_eq!(clean("a\u{202E}b".as_bytes()), "a\u{FFFD}b");
        assert_eq!(clean(b"bad \xff utf8"), "bad \u{FFFD} utf8");
    }
}
