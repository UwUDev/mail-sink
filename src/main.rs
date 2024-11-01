mod cli;
mod http;
mod smtp;
mod snowflake;
mod tests;

use crate::cli::*;
use clap::{CommandFactory, Parser};
use clap_help::Printer;
use sled::Db;
use std::error::Error;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
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
            .print_help();

        print_api_usage();
        return Ok(());
    }

    let tls_config = Arc::new(smtp::load_tls_config()?);
    let db = Arc::new(Mutex::new(sled::open("db")?));

    let db_clone = db.clone();
    let tls_clone = tls_config.clone();
    args.smtp_port.split(',')
        .map(|port| port.parse::<u16>().expect("Wrong ports"))
        .for_each(|port| {
            let tls = tls_clone.clone();
            let db = db_clone.clone();
            task::spawn(
                    async move { run_smtp_service(tls, db, port).await },
                );
        });


    let db_clone = db.clone();
    let key = args.key.clone();
    let service_handle =
        task::spawn(async move { run_http_service(db_clone, args.http_ports, key.clone()).await });



    match args.lifetime {
        Some(lifetime) => {
            // spawn a new task, me don't need to wait for it
            task::spawn(run_cleaner_service(db, lifetime));
        }
        _ => {}
    }

    println!(
        "Panel: http://localhost:{}/panel?k={}",
        args.http_ports, args.key
    );

    // wait for all services to complete (it should never happen)

    let _ = tokio::try_join!(service_handle)?;


    eprintln!("All services have completed unexpectedly ...");

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
                        db.insert(mail.id.to_le_bytes(), bytes).unwrap();
                    }
                }
                Err(e) => {
                    println!("Error handling client {}: {:?}", addr, e);
                }
            }
        });
    }
}

async fn run_http_service(
    db: Arc<Mutex<Db>>,
    i: u16,
    key: String,
) -> Result<(), Box<dyn Error + Send + Sync>> {
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

async fn run_cleaner_service(
    db: Arc<Mutex<Db>>,
    lifetime: u16,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    loop {
        let db = db.clone();
        let db = db.lock().await;
        let mut keys = Vec::new();
        for key in db.iter() {
            keys.push(key.unwrap().0);
        }

        let mut count = 0;
        for key in keys {
            let mail = db.get(&key).unwrap().unwrap();
            let mail: smtp::mail::Mail = bincode::deserialize(&mail).unwrap();
            let current_millis = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis();

            if current_millis - mail.timestamp() > (lifetime as u128 * 60 * 1000) {
                db.remove(&key).unwrap();
                count += 1;
            }
        }

        drop(db);

        if count > 0 {
            println!("Cleaned {} emails", count);
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
    }
}
