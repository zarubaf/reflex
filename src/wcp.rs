//! Minimal WCP (Waveform Control Protocol) client for Surfer cursor synchronization.
//!
//! WCP uses JSON messages over TCP, delimited by null bytes (`\0`).
//! The stream is split: writer behind a Mutex for sends, reader in a background task.

use std::sync::Arc;

use async_net::TcpStream;
use futures_lite::io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use smol::lock::Mutex;

use crate::views::log_panel::LogBuffer;

/// A lightweight WCP client that connects to a Surfer waveform viewer.
pub struct WcpClient {
    writer: Arc<Mutex<WriteHalf<TcpStream>>>,
}

impl WcpClient {
    /// Connect to a Surfer instance, perform the greeting handshake,
    /// and spawn a background reader that logs incoming messages.
    pub async fn connect(addr: &str, log: LogBuffer) -> anyhow::Result<Self> {
        let stream = TcpStream::connect(addr).await?;
        let (reader, writer) = futures_lite::io::split(stream);
        let writer = Arc::new(Mutex::new(writer));

        // Send our greeting.
        {
            let mut w = writer.lock().await;
            let greeting = r#"{"type":"greeting","version":"0","commands":["set_cursor"]}"#;
            w.write_all(greeting.as_bytes()).await?;
            w.write_all(b"\0").await?;
            w.flush().await?;
        }

        // Spawn background reader that drains and logs all server messages.
        smol::spawn(async move {
            if let Err(e) = read_loop(reader, &log).await {
                log.push(format!("WCP reader: {}", e));
            }
        })
        .detach();

        Ok(Self { writer })
    }

    /// Send a `set_cursor` command with the given timestamp.
    pub async fn send_cursor(&self, timestamp: u64) -> anyhow::Result<()> {
        let json =
            format!(r#"{{"type":"command","command":"set_cursor","timestamp":{timestamp}}}"#);
        let mut w = self.writer.lock().await;
        w.write_all(json.as_bytes()).await?;
        w.write_all(b"\0").await?;
        w.flush().await?;
        Ok(())
    }
}

/// Background loop that reads null-byte delimited messages and logs them.
async fn read_loop(mut reader: ReadHalf<TcpStream>, log: &LogBuffer) -> anyhow::Result<()> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let n = reader.read(&mut byte).await?;
        if n == 0 {
            log.push("WCP: server closed connection");
            break;
        }
        if byte[0] == 0 {
            if !buf.is_empty() {
                let msg = String::from_utf8_lossy(&buf);
                log.push(format!("WCP recv: {}", msg));
                buf.clear();
            }
        } else {
            buf.push(byte[0]);
        }
    }
    Ok(())
}
