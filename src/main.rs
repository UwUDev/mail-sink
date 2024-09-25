mod cli;
mod http;
mod smtp;

use crate::cli::*;
use clap::{CommandFactory, Parser};
use clap_help::Printer;
use sled::Db;
use std::error::Error;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::task;
use tokio_rustls::rustls::ServerConfig;

type SharedError = Box<dyn Error + Send + Sync>;

#[tokio::main]
async fn main() -> Result<(), SharedError> {
    let args: Args = Args::parse();
    if args.help {
        Printer::new(Args::command())
            .with("introduction", INTRO)
            .without("author")
            .print_help();

        print_api_usage();
        return Ok(());
    }

    let tls_config = Arc::new(smtp::load_tls_config()?);
    let db = Arc::new(Mutex::new(sled::open("db")?));

    let db_clone = db.clone();
    let tls_clone = tls_config.clone();
    let smtp_handle =
        task::spawn(async move { run_smtp_service(tls_clone.clone(), db_clone, args.smtp_port).await });

    let db_clone = db.clone();
    let service_handle =
        task::spawn(async move { run_http_service(db_clone, args.http_port, args.key.clone()).await });

    let secondary_smtp_handle = if let Some(port) = args.secondary_smtp_port {
        let tls_config = tls_config.clone();
        let db = db.clone();
        Some(task::spawn(async move { run_smtp_service(tls_config, db, port).await }))
    } else {
        None
    };

    // wait for all services to complete (it should never happen)
    match secondary_smtp_handle {
        Some(handle) => {
            let _ = tokio::try_join!(smtp_handle, service_handle, handle)?;
        }
        None => {
            let _ = tokio::try_join!(smtp_handle, service_handle)?;
        }
    }

    Ok(())
}

async fn run_smtp_service(
    tls_config: Arc<ServerConfig>,
    db: Arc<Mutex<Db>>,
    port: u16,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // bind the TCP listener to the address
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    println!("SMTP server running on port {}", port);

    loop {
        // accept a new incoming TCP connection
        let (socket, addr) = listener.accept().await?;
        println!("New client connected: {}", addr);

        // clone the TLS configuration for the spawned task
        let tls_config = tls_config.clone();
        let db = db.clone();

        // spawn a new task to handle the client
        tokio::spawn(async move {
            let result = smtp::handle_client(socket, tls_config, addr).await;
            match result {
                Ok(mail) => {
                    if mail.from.len() > 0 && mail.to.len() > 0 && mail.data.len() > 20 {
                        let db = db.lock().await;
                        let bytes = bincode::serialize(&mail).unwrap();
                        let to = mail.to.iter().next().unwrap().to_owned();
                        db.insert(to, bytes).unwrap();
                    }
                }
                Err(e) => {
                    println!("Error handling client {}: {:?}", addr, e);
                }
            }
        });
    }
}

async fn run_http_service(db: Arc<Mutex<Db>>, i: u16, key: String) -> Result<(), Box<dyn Error + Send + Sync>> {
    // bind the TCP listener to the address
    let listener = TcpListener::bind(format!("0.0.0.0:{}", i)).await?;
    println!("HTTP server running on port {}", i);

    loop {
        // accept a new incoming TCP connection
        let (socket, addr) = listener.accept().await?;
        println!("New client connected on port 8080: {}", addr);

        // handle the connection (implement your service logic here)
        let db = db.clone();
        let key = key.clone();
        tokio::spawn(async move {
            if let Err(e) = http::handle_client(socket, db, key.as_str()).await {
                println!("Error handling client {}: {:?}", addr, e);
            }
        });
    }
}
