use psutil::process::Process;
use serde_json::{json, Value};
use sled::Db;
use std::collections::HashMap;
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use sysinfo::{Disks, System};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::TcpStream;

use tokio::sync::{Mutex as AsyncMutex, Mutex};

use crate::smtp::mail::Mail;
use url::form_urlencoded;
use url::Url;

#[derive(Debug, PartialEq, Eq, Hash)]
enum Method {
    GET,
    POST,
    PUT,
    DELETE,
}

impl Method {
    fn from_str(method: &str) -> Option<Method> {
        match method.to_uppercase().as_str() {
            "GET" => Some(Method::GET),
            "POST" => Some(Method::POST),
            "PUT" => Some(Method::PUT),
            "DELETE" => Some(Method::DELETE),
            _ => None,
        }
    }
}

// Define a type alias for the handler function
type Handler = Box<
    dyn Fn(
            Request,
            Arc<AsyncMutex<BufWriter<tokio::net::tcp::OwnedWriteHalf>>>,
            Arc<Mutex<Db>>,
        )
            -> Pin<Box<dyn Future<Output = Result<(), Box<dyn Error + Send + Sync>>> + Send>>
        + Send
        + Sync,
>;

// Define a simple Request struct
#[allow(dead_code)] // will be used later
struct Request {
    method: Method,
    path: String,
    query: HashMap<String, String>,
    params: HashMap<String, String>,
}

pub(crate) async fn handle_client(
    stream: TcpStream,
    db: Arc<Mutex<Db>>,
    key: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let (reader, writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let writer = Arc::new(AsyncMutex::new(BufWriter::new(writer)));

    // Read the request line
    let mut request_line = String::new();
    let bytes_read = reader.read_line(&mut request_line).await?;
    if bytes_read == 0 {
        return Ok(());
    }

    // Parse the request line
    let request_line = request_line.trim_end();
    let mut parts = request_line.split_whitespace();
    let method_str = parts.next();
    let path_and_query = parts.next();
    let _version = parts.next();

    if let (Some(method_str), Some(path_and_query)) = (method_str, path_and_query) {
        // parse the method
        let method = Method::from_str(method_str);
        if method.is_none() {
            // Method isn't Allowed
            writer.lock().await.get_mut().shutdown().await?;
            return Ok(());
        }
        let method = method.unwrap();

        // parse the URL to handle path and query parameters
        let url = Url::parse(&format!("http://localhost{}", path_and_query))?;

        let path = url.path().to_string(); //url decode the path
        let path = percent_encoding::percent_decode_str(&path)
            .decode_utf8()
            .unwrap()
            .to_string();

        let query_pairs = form_urlencoded::parse(url.query().unwrap_or("").as_bytes())
            .into_owned()
            .collect::<HashMap<String, String>>();

        // check if the key is provided and valid before proceeding
        if let Some(k) = query_pairs.get("k") {
            if k != key {
                // 403 Forbidden, just close the connection without any response to avoid leaking information
                writer.lock().await.get_mut().shutdown().await?;
                return Ok(());
            }
        } else {
            // 401 Unauthorized, just close the connection without any response to avoid leaking information
            writer.lock().await.get_mut().shutdown().await?;
            return Ok(());
        }

        let routes = build_routes();
        if let Some((handler, params)) = find_handler(&routes, &method, &path) {
            let request = Request {
                method,
                path,
                query: query_pairs,
                params,
            };
            handler(request, writer.clone(), db.clone()).await?;
        } else {
            let mut writer = writer.lock().await;
            writer.write_all(b"HTTP/1.1 404 Not Found\r\n\r\n").await?;
            writer.flush().await?;
        }
    } else {
        // bad request (most likely a skill issue)
        let mut writer = writer.lock().await;
        writer
            .write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n")
            .await?;
        writer.flush().await?;
    }

    Ok(())
}

// function to build the routing table
fn build_routes() -> Vec<(Method, String, Handler)> {
    vec![
        (
            Method::GET,
            "/mails/:mail_id".to_string(),
            Box::new(|request, writer, db| Box::pin(get_mail_handler(request, writer, db))),
        ),
        (
            Method::GET,
            "/mails/to/:email".to_string(),
            Box::new(|request, writer, db| Box::pin(get_mail_from_to_handler(request, writer, db, true))),
        ),
        (
            Method::GET,
            "/mails/from/:email".to_string(),
            Box::new(|request, writer, db| Box::pin(get_mail_from_to_handler(request, writer, db, false))),
        ),
        (
            Method::DELETE,
            "/mails/:mail_id".to_string(),
            Box::new(|request, writer, db| Box::pin(delete_mail_handler(request, writer, db))),
        ),
        (
            Method::GET,
            "/mails".to_string(),
            Box::new(|request, writer, db| Box::pin(get_mails_handler(request, writer, db))),
        ),
        (
            Method::DELETE,
            "/mails".to_string(),
            Box::new(|_, writer, db| Box::pin(delete_mails_handler(writer, db))),
        ),
        (
            Method::GET,
            "/info".to_string(),
            Box::new(|_, writer, db| Box::pin(info_handler(writer, db))),
        ),
        (
            Method::GET,
            "/preview/:mail_id".to_string(),
            Box::new(|request, writer, db| Box::pin(preview_mail_handler(request, writer, db))),
        ),
        (
            Method::GET,
            "/panel".to_string(),
            Box::new(|_, writer, _| Box::pin(panel_handler(writer))),
        ),
    ]
}

// function to find the appropriate handler
fn find_handler<'a>(
    routes: &'a [(Method, String, Handler)],
    method: &Method,
    request_path: &str,
) -> Option<(&'a Handler, HashMap<String, String>)> {
    for (route_method, route_path, handler) in routes {
        if method == route_method {
            if let Some(params) = match_path(route_path, request_path) {
                return Some((handler, params));
            }
        }
    }
    None
}

// function to match paths with parameters
fn match_path(route_path: &str, request_path: &str) -> Option<HashMap<String, String>> {
    let route_parts: Vec<&str> = route_path.trim_end_matches('/').split('/').collect();
    let request_parts: Vec<&str> = request_path.trim_end_matches('/').split('/').collect();

    if route_parts.len() != request_parts.len() {
        return None;
    }

    let mut params = HashMap::new();

    for (route_part, request_part) in route_parts.iter().zip(request_parts.iter()) {
        if route_part.starts_with(':') {
            let name = route_part.trim_start_matches(':');
            params.insert(name.to_string(), request_part.to_string());
        } else if route_part != request_part {
            return None;
        }
    }

    Some(params)
}

//     HANDLERS     //

async fn get_mail_handler(
    request: Request,
    writer: Arc<AsyncMutex<BufWriter<tokio::net::tcp::OwnedWriteHalf>>>,
    db: Arc<Mutex<Db>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mail_id = request.params.get("mail_id").unwrap();

    let mail_id = u128::from_str_radix(mail_id, 10).map_err(|_| {
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid mail_id",
        )) as Box<dyn Error + Send + Sync>
    })?;

    let db = db.lock().await;
    let result = db.get(mail_id.to_le_bytes());

    let mut writer = writer.lock().await;

    if let Ok(Some(data)) = result {
        let mail: Mail = bincode::deserialize(&data)?;
        let mut json = serde_json::to_value(&mail)?;
        json["body"] = Value::String(mail.parse_body());
        Value::Number(serde_json::Number::from_str(&mail.timestamp().to_string()).unwrap());
        let json = serde_json::to_string(&json)?;

        writer.write_all(b"HTTP/1.1 200 OK\r\n").await?;
        writer
            .write_all(b"Content-Type: application/json\r\n")
            .await?;
        writer
            .write_all(format!("Content-Length: {}\r\n", json.len()).as_bytes())
            .await?;
        writer.write_all(b"\r\n").await?;
        writer.write_all(json.as_bytes()).await?;
    } else {
        writer.write_all(b"HTTP/1.1 404 Not Found\r\n\r\n").await?;
    }

    writer.flush().await?;
    Ok(())
}

async fn delete_mail_handler(
    request: Request,
    writer: Arc<AsyncMutex<BufWriter<tokio::net::tcp::OwnedWriteHalf>>>,
    db: Arc<Mutex<Db>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mail_id = request.params.get("mail_id").unwrap();
    let mail_id = u128::from_str_radix(mail_id, 10).map_err(|_| {
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid mail_id",
        )) as Box<dyn Error + Send + Sync>
    })?;

    let db = db.lock().await;
    let result = db.get(mail_id.to_le_bytes());

    let mut writer = writer.lock().await;

    if result.is_ok() {
        db.remove(mail_id.to_le_bytes()).unwrap();
        writer.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await?;
    } else {
        writer.write_all(b"HTTP/1.1 404 Not Found\r\n\r\n").await?;
    }
    writer.flush().await?;
    Ok(())
}

async fn get_mails_handler(
    request: Request,
    writer: Arc<AsyncMutex<BufWriter<tokio::net::tcp::OwnedWriteHalf>>>,
    db: Arc<Mutex<Db>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let limit = request
        .query
        .get("limit")
        .unwrap_or(&String::from("10"))
        .parse::<usize>()
        .unwrap();
    let offset = request
        .query
        .get("offset")
        .unwrap_or(&String::from("0"))
        .parse::<usize>()
        .unwrap();

    let db = db.lock().await;
    let mut iter = db.iter().rev();
    let mut mails = Vec::new();
    let mut count = 0;

    for _ in 0..offset {
        if iter.next().is_none() {
            break;
        }
    }

    while let Some(result) = iter.next() {
        let (_, data) = result?;
        let mail: Mail = bincode::deserialize(&data)?;
        mails.push(mail);
        count += 1;
        if count >= limit {
            break;
        }
    }

    let mut mails_json = Vec::new();
    for mail in &mails {
        let mut json: Value = serde_json::to_value(mail)?;
        let parsed_body = mail.parse_body();
        json["body"] = Value::String(parsed_body);
        json["timestamp"] =
            Value::Number(serde_json::Number::from_str(&mail.timestamp().to_string()).unwrap());
        mails_json.push(json);
    }
    let json = serde_json::to_string(&mails_json)?;

    let mut writer = writer.lock().await;
    writer.write_all(b"HTTP/1.1 200 OK\r\n").await?;
    writer
        .write_all(b"Content-Type: application/json\r\n")
        .await?;
    writer
        .write_all(format!("Content-Length: {}\r\n", json.len()).as_bytes())
        .await?;
    writer.write_all(b"\r\n").await?;
    writer.write_all(json.as_bytes()).await?;

    writer.flush().await?;
    Ok(())
}

async fn delete_mails_handler(
    writer: Arc<AsyncMutex<BufWriter<tokio::net::tcp::OwnedWriteHalf>>>,
    db: Arc<Mutex<Db>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let db = db.lock().await;
    db.clear().unwrap();

    let mut writer = writer.lock().await;
    writer.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await?;
    writer.flush().await?;
    Ok(())
}

async fn info_handler(
    writer: Arc<AsyncMutex<BufWriter<tokio::net::tcp::OwnedWriteHalf>>>,
    db: Arc<Mutex<Db>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let db = db.lock().await;
    let count = db.len();

    let database_disk_usage = db.size_on_disk().unwrap();
    let pid = std::process::id();
    let mut process = Process::new(pid).unwrap();

    let mem_info = process.memory_info().unwrap();
    let mem_usage = mem_info.rss();

    let mut system = System::new();
    system.refresh_memory();
    system.refresh_cpu_usage();

    let machine_memory_usage = system.used_memory();
    let machine_memory_total = system.total_memory();

    // this is a bit tricky, but it's needed to get the correct CPU usage because
    // refresh_cpu_usage() consumes lots of CPU for a really short time
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let cpu_usage = process.cpu_percent().unwrap();

    let machine_cpu_usage = system.global_cpu_usage();

    let max_cpu_usage = num_cpus::get() as f32 * 100.0;

    let disks = Disks::new_with_refreshed_list();
    let disk_usage: u64 = disks
        .iter()
        .map(|disk| disk.total_space() - disk.available_space())
        .sum();
    let free_space: u64 = disks.iter().map(|disk| disk.available_space()).sum();

    let json = json!({
        "mail_count": count,
        "database_disk_usage": database_disk_usage,
        "memory_usage": mem_usage,
        "machine_memory_usage": machine_memory_usage,
        "machine_memory_total": machine_memory_total,
        "cpu_usage": cpu_usage,
        "machine_cpu_usage": machine_cpu_usage,
        "max_cpu_usage": max_cpu_usage,
        "disk_usage": disk_usage,
        "free_space": free_space,
    });

    let json = serde_json::to_string(&json)?;

    let mut writer = writer.lock().await;
    writer.write_all(b"HTTP/1.1 200 OK\r\n").await?;
    writer
        .write_all(b"Content-Type: application/json\r\n")
        .await?;
    writer
        .write_all(format!("Content-Length: {}\r\n", json.len()).as_bytes())
        .await?;
    writer.write_all(b"\r\n").await?;

    writer.write_all(json.as_bytes()).await?;

    writer.flush().await?;
    Ok(())
}

async fn preview_mail_handler(
    request: Request,
    writer: Arc<AsyncMutex<BufWriter<tokio::net::tcp::OwnedWriteHalf>>>,
    db: Arc<Mutex<Db>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mail_id = request.params.get("mail_id").unwrap();
    let mail_id = u128::from_str_radix(mail_id, 10).map_err(|_| {
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid mail_id",
        )) as Box<dyn Error + Send + Sync>
    })?;

    let db = db.lock().await;
    let result = db.get(mail_id.to_le_bytes());

    let mut writer = writer.lock().await;

    if result.is_err() {
        writer
            .write_all(b"HTTP/1.1 500 Internal Server Error\r\n\r\n")
            .await?;
        writer.flush().await?;
        return Ok(());
    } else if result.unwrap().is_none() {
        writer.write_all(b"HTTP/1.1 404 Not Found\r\n\r\n").await?;
        writer.flush().await?;
        return Ok(());
    }

    // return preview.html
    let body = include_bytes!("pages/preview.html");
    writer.write_all(b"HTTP/1.1 200 OK\r\n").await?;
    writer.write_all(b"Content-Type: text/html\r\n").await?;
    writer
        .write_all(format!("Content-Length: {}\r\n", body.len()).as_bytes())
        .await?;

    writer.write_all(b"\r\n").await?;
    writer.write_all(body).await?;
    writer.flush().await?;
    Ok(())
}

async fn panel_handler(
    writer: Arc<AsyncMutex<BufWriter<tokio::net::tcp::OwnedWriteHalf>>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let body = include_bytes!("pages/panel.html");

    let mut writer = writer.lock().await;
    writer.write_all(b"HTTP/1.1 200 OK\r\n").await?;
    writer.write_all(b"Content-Type: text/html\r\n").await?;
    writer
        .write_all(format!("Content-Length: {}\r\n", body.len()).as_bytes())
        .await?;

    writer.write_all(b"\r\n").await?;
    writer.write_all(body).await?;
    writer.flush().await?;
    Ok(())
}

async fn get_mail_from_to_handler(
    request: Request,
    writer: Arc<AsyncMutex<BufWriter<tokio::net::tcp::OwnedWriteHalf>>>,
    db: Arc<Mutex<Db>>,
    to: bool,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let email_filter = request.params.get("email").unwrap().to_lowercase();
    let limit = request
        .query
        .get("limit")
        .unwrap_or(&String::from("10"))
        .parse::<usize>()
        .unwrap();
    let offset = request
        .query
        .get("offset")
        .unwrap_or(&String::from("0"))
        .parse::<usize>()
        .unwrap();

    let db = db.lock().await;
    let mut iter = db.iter().rev();
    let mut mails = Vec::new();
    let mut count = 0;

    for _ in 0..offset {
        if iter.next().is_none() {
            break;
        }
    }

    while let Some(result) = iter.next() {
        let (_, data) = result?;
        let mail: Mail = bincode::deserialize(&data)?;

        if to {
            if mail.to.iter().any(|to_email| to_email.to_lowercase() == email_filter) {
                mails.push(mail);
                count += 1;
                if count >= limit {
                    break;
                }
            }
        } else {
            if mail.from.iter().any(|to_email| to_email.to_lowercase() == email_filter) {
                mails.push(mail);
                count += 1;
                if count >= limit {
                    break;
                }
            }
        }
    }

    let mut mails_json = Vec::new();
    for mail in &mails {
        let mut json: Value = serde_json::to_value(mail)?;
        let parsed_body = mail.parse_body();
        json["body"] = Value::String(parsed_body);
        json["timestamp"] =
            Value::Number(serde_json::Number::from_str(&mail.timestamp().to_string()).unwrap());
        mails_json.push(json);
    }
    let json = serde_json::to_string(&mails_json)?;

    let mut writer = writer.lock().await;
    writer.write_all(b"HTTP/1.1 200 OK\r\n").await?;
    writer
        .write_all(b"Content-Type: application/json\r\n")
        .await?;
    writer
        .write_all(format!("Content-Length: {}\r\n", json.len()).as_bytes())
        .await?;
    writer.write_all(b"\r\n").await?;
    writer.write_all(json.as_bytes()).await?;

    writer.flush().await?;
    Ok(())
}