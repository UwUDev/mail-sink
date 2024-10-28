use clap::Parser;
use colored::Colorize;

#[derive(Parser, Debug)]
#[command(name = "mail-sink", author, version, about, disable_help_flag = true)]
pub struct Args {
    #[arg(long, short)]
    pub help: bool,

    #[arg(
        short = 'p',
        long,
        default_value = "2525",
        value_name = "SMTP PORTS",
        help = "Example: `25,587,465`"
    )]
    pub smtp_port: String,


    #[arg(long, default_value = "8080", value_name = "HTTP PORT")]
    pub http_ports: u16,

    #[arg(
        short,
        long,
        default_value = "prouteur",
        help = "The key to access the API"
    )]
    pub key: String,

    #[arg(
        short,
        long,
        help = "The lifetime of an email in the database in minutes",
        value_name = "LIFETIME IN MINUTES"
    )]
    pub lifetime: Option<u16>,
}

pub static INTRO: &str = "
Mail Sink is a simple mail server that accepts any incomingemail and stores it in a database. It
provides an HTTP API to retrieve and delete the stored emails. It can be used for testing email
sending functionality in your application, or mass email verification / account generation.


**Note:**
Port numbers under 1024 require root privileges. If you want to use a port number lower than 1024
you can use a reverse proxy like Nginx or Apache to forward the traffic to the Mail Sink server
running on a higher port number.
";

pub fn print_api_usage() {
    println!("{}", "API access:".bold());
    println!("The HTTP API is accessible by adding ?k=your_key to the URL.");
    println!();
    println!(
        "- {} {}                          Retrieve all stored emails (JSON format)",
        "GET".blue(),
        "/mails".bold()
    );
    println!(
        "  • {}: ?limit and ?offset for pagination",
        "Parameters".bright_black()
    );
    println!(
        "- {} {}               Retrieve a specific email (JSON format)",
        "GET".blue(),
        "/mails/<email_id>".bold()
    );
    println!(
        "- {} {}       Retrieve all emails to (JSON format)",
        "GET".blue(),
        "/mails/to/<email_address>".bold()
    );
    println!(
        "  • {}: ?limit and ?offset for pagination",
        "Parameters".bright_black()
    );
    println!(
        "- {} {}     Retrieve all emails from (JSON format)",
        "GET".blue(),
        "/mails/from/<email_address>".bold()
    );
    println!(
        "  • {}: ?limit and ?offset for pagination",
        "Parameters".bright_black()
    );
    println!(
        "- {} {}            Delete a specific email",
        "DELETE".red(),
        "/mails/<email_id>".bold()
    );
    println!(
        "- {} {}                       Delete {}",
        "DELETE".red(),
        "/mails".bold(),
        "all stored emails".bold()
    );
    println!(
        "- {} {}    Delete all emails to",
        "DELETE".red(),
        "/mails/to/<email_address>".bold()
    );
    println!(
        "- {} {}  Delete all emails from",
        "DELETE".red(),
        "/mails/from/<email_address>".bold()
    );
}
