use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
    sync::mpsc,
};
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let stream = TcpStream::connect("127.0.0.1:9999").await?;
    let (reader, mut writer) = stream.into_split();

    let mut reader = BufReader::new(reader);
    let (tx, mut rx) = mpsc::channel::<String>(10);

    // Spawn task to read from stdin
    tokio::spawn(async move {
        let stdin = tokio::io::stdin();
        let mut stdin_reader = BufReader::new(stdin).lines();

        while let Ok(Some(line)) = stdin_reader.next_line().await {
            if tx.send(line).await.is_err() {
                break;
            }
        }
    });

    // Spawn task to read from socket
    tokio::spawn(async move {
        loop {
            let mut len_buf = [0u8; 8];
            if reader.read_exact(&mut len_buf).await.is_err() {
                eprintln!("Connection closed by server.");
                break;
            }

            let msg_len = u64::from_be_bytes(len_buf) as usize;
            let mut buffer = vec![0u8; msg_len];

            if reader.read_exact(&mut buffer).await.is_err() {
                eprintln!("Failed to read message.");
                break;
            }

            if let Ok(message) = String::from_utf8(buffer) {
                println!("Received: {}", message);
            } else {
                eprintln!("Received invalid UTF-8 data.");
            }
        }
    });

    // Handle sending messages to the server
    while let Some(msg) = rx.recv().await {
        let msg_bytes = msg.as_bytes();
        let msg_len = msg_bytes.len() as u64;
        let mut packet = Vec::with_capacity(8 + msg_bytes.len());
        packet.extend_from_slice(&msg_len.to_be_bytes());
        packet.extend_from_slice(msg_bytes);

        eprintln!("Sending payload of {} bytes. Network byte count {}", msg_len, packet.len());
        if writer.write_all(&packet).await.is_err() {
            eprintln!("Failed to send message.");
            break;
        }
    }

    Ok(())
}
