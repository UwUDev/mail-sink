use crate::SharedError;
use mailparse::parse_mail;
use rustls_pemfile::{certs, pkcs8_private_keys};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::error::Error;
use std::fs::File;
use std::io::BufReader as StdBufReader;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_rustls::rustls::{Certificate, PrivateKey, ServerConfig};
use tokio_rustls::TlsAcceptor;

#[derive(Default, Serialize, Deserialize)]
pub struct Mail {
    pub from: HashSet<String>,
    pub to: HashSet<String>,
    pub data: String,
    pub id: u128,
}

impl Mail {
    pub fn parse_body(&self) -> String {
        let mail = match parse_mail(self.data.as_bytes()) {
            Ok(parsed) => parsed,
            // return raw body if parsing fails
            Err(_) => {
                let mut body = self.data.clone();
                // after the headers
                if let Some(index) = body.find("\r\n\r\n") {
                    body = body[index + 4..].to_string();
                }
                return body;
            }
        };

        // check if the email is multipart
        if mail.subparts.is_empty() {
            // not multipart, return the body as is
            mail.get_body().unwrap_or_else(|_| String::new())
        } else {
            // prioritize 'text/html' parts
            for part in &mail.subparts {
                let content_type = part.ctype.mimetype.to_lowercase();
                if content_type == "text/html" {
                    // Return the HTML part's body
                    return part.get_body().unwrap_or_else(|_| String::new());
                }
            }
            // no 'text/html' part found, return the first multipart's body
            mail.subparts[0]
                .get_body()
                .unwrap_or_else(|_| String::new())
        }
    }

    pub fn timestamp(&self) -> u128 {
        crate::snowflake::to_timestamp(self.id)
    }

    pub fn new(from: HashSet<String>, to: HashSet<String>, data: String) -> Self {
        Self {
            from,
            to,
            data,
            id: crate::snowflake::next(),
        }
    }
}
pub(crate) async fn handle_client(
    stream: TcpStream,
    tls_config: Arc<ServerConfig>,
    peer_addr: SocketAddr,
) -> Result<Mail, SharedError> {
    let (reader, writer) = stream.into_split();

    let mut reader = BufReader::new(reader);
    let mut writer = writer;

    // greeting
    writer.write_all(b"220 mail-sink\r\n").await?;

    let mut from = HashSet::new();
    let mut to = HashSet::new();
    let mut body = String::new();

    loop {
        let mut line = String::new();

        let bytes_read = reader.read_line(&mut line).await?;
        if bytes_read == 0 {
            // connection closed :((((
            break;
        }

        let command = line.trim_end();
        let command_upper = command.to_uppercase();

        if command_upper.starts_with("EHLO") || command_upper.starts_with("HELO") {
            writer.write_all(b"250-localhost\r\n").await?;
            // STARTTLS capability
            writer.write_all(b"250-STARTTLS\r\n").await?;
            writer.write_all(b"250 OK\r\n").await?;
        } else if command_upper.starts_with("STARTTLS") {
            writer.write_all(b"220 Ready to start TLS\r\n").await?;
            writer.flush().await?;

            // Reunite the read and write halves
            let stream = reader.into_inner().reunite(writer)?;
            // Upgrade to TLS
            let acceptor = TlsAcceptor::from(tls_config.clone());
            let tls_stream = acceptor.accept(stream).await?;

            let tls_mail = handle_tls_client(tls_stream).await;
            match tls_mail {
                Ok(m) => {
                    from = m.from;
                    to = m.to;
                    body = m.data;
                }
                Err(e) => {
                    println!("Error handling TLS client {}: {:?}", peer_addr, e);
                }
            }
            break;
        } else if command_upper.starts_with("MAIL FROM") {
            from.insert(command[10..].to_string().replace("<", "").replace(">", ""));
            writer.write_all(b"250 OK\r\n").await?;
        } else if command_upper.starts_with("RCPT TO") {
            to.insert(command[8..].to_string().replace("<", "").replace(">", ""));
            writer.write_all(b"250 OK\r\n").await?;
        } else if command_upper == "DATA" {
            writer
                .write_all(b"354 End data with <CR><LF>.<CR><LF>\r\n")
                .await?;

            // email data processing
            let mut data = String::new();
            loop {
                line.clear();
                let bytes_read = reader.read_line(&mut line).await?;
                if bytes_read == 0 {
                    // connection closed unexpectedly
                    break;
                }
                let trimmed_line = line.trim_end();
                if trimmed_line == "." {
                    break;
                }
                data.push_str(&line);
            }

            let (f, t) = get_data_from_to(&data);
            f.iter().for_each(|s| {
                from.insert(s.clone());
            });
            t.iter().for_each(|s| {
                to.insert(s.clone());
            });

            body = data.clone();
            writer.write_all(b"250 OK\r\n").await?;
        } else if command_upper == "QUIT" {
            // reunite the read and write halves
            let mut stream = reader.into_inner().reunite(writer)?;
            // close the connection
            stream.shutdown().await?;
            break;
        } else {
            writer.write_all(b"502 Command not implemented\r\n").await?;
        }
    }

    Ok(Mail::new(from, to, body))
}

async fn handle_tls_client(
    stream: tokio_rustls::server::TlsStream<TcpStream>,
    //peer_addr: SocketAddr,
) -> Result<Mail, Box<dyn Error>> {
    let (read_half, write_half) = tokio::io::split(stream);
    let mut reader = BufReader::new(read_half);
    let mut writer = write_half;

    let mut from = HashSet::new();
    let mut to = HashSet::new();
    let mut body = String::new();

    loop {
        let mut line = String::new();

        let bytes_read = reader.read_line(&mut line).await?;
        if bytes_read == 0 {
            // connection closed :((((
            break;
        }

        let command = line.trim_end();
        let command_upper = command.to_uppercase();

        if command_upper.starts_with("EHLO") || command_upper.starts_with("HELO") {
            writer.write_all(b"250-localhost\r\n").await?;
            writer.write_all(b"250 OK\r\n").await?;
        } else if command_upper.starts_with("MAIL FROM") {
            from.insert(command[10..].to_string().replace("<", "").replace(">", ""));
            writer.write_all(b"250 OK\r\n").await?;
        } else if command_upper.starts_with("RCPT TO") {
            to.insert(command[8..].to_string().replace("<", "").replace(">", ""));
            writer.write_all(b"250 OK\r\n").await?;
        } else if command_upper == "DATA" {
            writer
                .write_all(b"354 End data with <CR><LF>.<CR><LF>\r\n")
                .await?;

            // email data processing
            let mut data = String::new();
            loop {
                line.clear();
                let bytes_read = reader.read_line(&mut line).await?;
                if bytes_read == 0 {
                    // connection closed unexpectedly
                    break;
                }
                let trimmed_line = line.trim_end();
                if trimmed_line == "." {
                    break;
                }
                data.push_str(&line);
            }

            let (f, t) = get_data_from_to(&data);
            f.iter().for_each(|s| {
                from.insert(s.clone());
            });
            t.iter().for_each(|s| {
                to.insert(s.clone());
            });

            body = data.clone();

            writer.write_all(b"250 OK\r\n").await?;
        } else if command_upper == "QUIT" {
            break;
        } else {
            writer.write_all(b"502 Command not implemented\r\n").await?;
        }
    }

    Ok(Mail::new(from, to, body))
}

pub fn load_tls_config() -> Result<ServerConfig, Box<dyn Error + Send + Sync>> {
    // load the TLS certificate and private key files
    let cert_file = &mut StdBufReader::new(File::open("cert.pem")?);
    let key_file = &mut StdBufReader::new(File::open("key.pem")?);

    // cert pem
    let cert_chain = certs(cert_file)
        .map_err(|_| "Failed to read certificate file")?
        .into_iter()
        .map(Certificate)
        .collect();

    // keys pem
    let mut keys = pkcs8_private_keys(key_file)
        .map_err(|_| "Failed to read key file")?
        .into_iter()
        .map(PrivateKey)
        .collect::<Vec<_>>();

    if keys.is_empty() {
        return Err(From::from("No private keys found in key.pem"));
    }

    let config = ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(cert_chain, keys.remove(0))?;

    Ok(config)
}

pub fn get_data_from_to(data: &String) -> (HashSet<String>, HashSet<String>) {
    let mut lines = data.lines();
    let mut from_set = HashSet::new();
    let mut to_set = HashSet::new();
    while let Some(line) = lines.next() {
        if line.to_lowercase().starts_with("from:") {
            let from = line.splitn(2, ':').nth(1).unwrap();
            from.split(",")
                .map(|s| s.trim())
                .for_each(|s| {
                    if s.contains("<") {
                        from_set.insert(s.split("<").nth(1).unwrap().split(">").nth(0).unwrap().to_string());
                    } else {
                        from_set.insert(s.to_string());
                    }
                });
        }
        if line.to_lowercase().starts_with("to:") {
            let to = line.splitn(2, ':').nth(1).unwrap();
            to.split(",")
                .map(|s| s.trim())
                .for_each(|s| {
                    if s.contains("<") {
                        to_set.insert(s.split("<").nth(1).unwrap().split(">").nth(0).unwrap().to_string());
                    } else {
                        to_set.insert(s.to_string());
                    }
                });
        }
    }

    (from_set, to_set)
}

