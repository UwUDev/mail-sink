use clap::Parser;
use colored::Colorize;

#[derive(Parser, Debug)]
#[command(name = "mail-sink", author, version, about, disable_help_flag = true)]
pub struct Args {
    #[arg(long, short)]
    pub help: bool,

    #[arg(short = 'p', long, default_value = "2525", value_name = "SMTP PORT")]
    pub smtp_port: u16,

    #[arg(
        short,
        long,
        help = "Not implemented yet.",
        value_name = "SMTP PORT"
    )]
    pub secondary_smtp_port: Option<u16>, // TODO: Implement this

    #[arg(long, default_value = "8080", value_name = "HTTP PORT")]
    pub http_port: u16,

    #[arg(
        short,
        long,
        default_value = "prouteur",
        help = "The key to access the API"
    )]
    pub key: String,
}

pub static INTRO: &str = "
Mail Sink is a simple mail server that accepts any incoming
email and stores it in a database. It provides an HTTP API
to retrieve and delete the stored emails. It can be used for
testing email sending functionality in your application, or
mass email verification / account generation.


**Note:**
Port numbers under 1024 require root privileges. If you want
to use a port number lower than 1024, you can use a reverse
proxy like Nginx or Apache to forward the traffic to the Mail
Sink server running on a higher port number.
";

pub fn print_api_usage() {
    println!("{}", "API access:".bold());
    println!("The HTTP API is accessible by adding ?key=your_key to the URL.");
    println!();
    println!(
        "- {} {}             Retrieve all stored emails (JSON format)",
        "GET".blue(),
        "/mails".bold()
    );
    println!(
        "- {} {}     Retrieve a specific email (JSON format)",
        "GET".blue(),
        "/mails/<email>".bold()
    );
    println!(
        "- {} {}  Delete a specific email",
        "DELETE".red(),
        "/mails/<email>".bold()
    );
    println!(
        "- {} {}          Delete {} stored emails",
        "DELETE".red(),
        "/mails".bold(),
        "all".bold()
    );
}
