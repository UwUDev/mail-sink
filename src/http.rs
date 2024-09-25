use sled::Db;
use std::error::Error;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

const KEY: &str = "feur";

use crate::smtp::Mail;
use url::form_urlencoded;
use url::Url;

pub(crate) async fn handle_client(
    stream: TcpStream,
    db: Arc<Mutex<Db>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let (reader, writer) = stream.into_split();

    let mut reader = BufReader::new(reader);
    let mut writer = writer;
    let mut line = String::new();
    let bytes_read = reader.read_line(&mut line).await?;
    if bytes_read == 0 {
        // connection closed (very sad moment)
        return Ok(());
    }

    let command = line.trim_end().split(' ');
    // the second element of the split is the command (GET command HTTP/x.x)
    let command = command.collect::<Vec<&str>>()[1];
    let command = "http://localhost:8080/".to_string() + command;
    println!("Received command: {}", command);
    let url = Url::parse(&command)?;

    let query = url.query().unwrap_or("");
    let params: Vec<(String, String)> = form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();

    let mail = params
        .iter()
        .find(|(key, _)| key == "mail")
        .map(|(_, value)| value.clone());

    let k = params
        .iter()
        .find(|(key, _)| key == "k")
        .map(|(_, value)| value.clone());

    // false if not present
    let delete = params.iter().find(|(key, _)| key == "delete").is_some();

    if k.is_none() || mail.is_none() {
        return Ok(());
    }

    let mail = mail.unwrap();
    let k = k.unwrap();

    if k != KEY {
        return Ok(());
    }

    let db = db.lock().await;
    let result = db.get(mail.as_bytes());
    if result.is_ok() {
        let result = result.unwrap();
        if result.is_some() {
            let result = result.unwrap();
            let mail: Mail = bincode::deserialize(&result).unwrap();
            let json = serde_json::to_string(&mail).unwrap();

            // http response
            writer.write_all(b"HTTP/1.1 200 OK\r\n").await?;
            writer
                .write_all(b"Content-Type: application/json\r\n")
                .await?;
            let len = json.len().to_string();
            writer.write_all(b"Content-Length: ").await?;
            writer.write_all(len.as_bytes()).await?;
            writer.write_all(b"\r\n").await?;
            writer.write_all(b"\r\n").await?;
            writer.write_all(json.as_bytes()).await?;

            if delete {
                db.remove(mail.to.iter().next().unwrap().as_bytes())
                    .unwrap();
            }
        } else {
            writer.write_all(b"HTTP/1.1 404 Not Found\r\n").await?;
        }
    } else {
        writer
            .write_all(b"HTTP/1.1 500 Internal Server Error\r\n")
            .await?;
    }

    Ok(())
}
