use std::collections::HashSet;
use mailparse::parse_mail;
use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
pub struct Mail {
    pub from: HashSet<String>,
    pub to: HashSet<String>,
    // TODO: subject
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

pub fn get_data_from_to(data: &str) -> (HashSet<String>, HashSet<String>) {
    let mut from_set = HashSet::new();
    let mut to_set = HashSet::new();

    let mut current_header_name = String::new();
    let mut current_header_value = String::new();

    for line in data.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            // continuation line
            current_header_value.push(' ');
            current_header_value.push_str(line.trim());
        } else if let Some((name, value)) = parse_header_line(line) {
            process_header(&current_header_name, &current_header_value, &mut from_set, &mut to_set);
            current_header_name = name;
            current_header_value = value.to_string();
        }
    }

    process_header(&current_header_name, &current_header_value, &mut from_set, &mut to_set);

    (from_set, to_set)
}

fn parse_header_line(line: &str) -> Option<(String, &str)> {
    if let Some(index) = line.find(':') {
        let name = line[..index].trim().to_string();
        let value = line[index + 1..].trim();
        Some((name, value))
    } else {
        None // malformed line
    }
}

fn process_header(
    header_name: &str,
    header_value: &str,
    from_set: &mut HashSet<String>,
    to_set: &mut HashSet<String>,
) {
    if header_name.eq_ignore_ascii_case("From") {
        extract_emails(header_value, from_set);
    } else if header_name.eq_ignore_ascii_case("To") {
        extract_emails(header_value, to_set);
    }
}

fn extract_emails(header_value: &str, email_set: &mut HashSet<String>) {
    for email_str in header_value.split(',').map(str::trim) {
        if let Some(email) = extract_email_address(email_str) {
            email_set.insert(email);
        }
    }
}

fn extract_email_address(s: &str) -> Option<String> {
    if let Some(start) = s.find('<') {
        if let Some(end) = s.find('>') {
            Some(s[start + 1..end].to_string())
        } else {
            None // malformed email
        }
    } else {
        Some(s.to_string())
    }
}