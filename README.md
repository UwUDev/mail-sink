# Mail Sink

An ultra lightweight mail sink supporting TLS with HTTP API, embedded database and a panel. Mostly used to massively verify email addresses on some site or for account generation.

## Table of Contents
- [Overview](#overview)
- [Features](#features)
- [Installation](#installation)
- [Building](#building)
- [Usage](#usage)
  - [Options](#options)
- [Panel](#panel)
- [Open mail](#open-mail)
- [API Access](#api-access)
- [Notes](#notes)

## Overview
Mail Sink is a simple mail server that accepts any incoming email and stores it in a database. It provides an HTTP API to retrieve and delete the stored emails. It can be used for testing email sending functionality in your application, or for mass email verification and account generation.

## Features
- Supports incoming email storage.
- HTTP API for retrieval and deletion of stored emails.
- TLS support.
- Embedded database.
- Useful panel
- Mails preview
- Really low memory and CPU usage.

## Installation

You can download the latest release from the [releases page](https://github.com/UwUDev/mail-sink/releases). Extract the binary and run it from your terminal.

## Building 

To build Mail Sink from source, follow these steps:
1. Ensure you have [Rust](https://www.rust-lang.org/) installed.
2. Clone the repository:
    ```sh
    git clone https://github.com/UwUDev/mail-sink.git
    ```
3. Navigate to the project directory:
    ```sh
    cd mail-sink
    ```
4. Build the project:
    ```sh
    cargo build --release
    ```
5. The built binary will be in `target/release/`.

## Usage

Run Mail Sink with default settings:
```sh
./mail-sink
```

Run Mail Sink with specific options:
```sh
./mail-sink [options]
```

### Options:
| short | long                   | value     | description                                         |
|-------|------------------------|-----------|-----------------------------------------------------|
| -h    | --help                 |           | Show help message.                                  |
| -p    | --smtp-port            | SMTP PORT | Set the SMTP port. Default: `2525`                  |
| -s    | --secondary-smtp-port  | SMTP PORT | Bind the SMTP server to a secondary port.           |
|       | --http-port            | HTTP PORT | Set the HTTP port. Default: `8080`                  |
| -k    | --key                  | KEY       | The key to access the API. Default: `prouteur`      |
| -V    | --version              |           | Print version.                                      |

## Panel
The panel is accessible via `/panel?k=your_key`

![image](https://github.com/user-attachments/assets/9163df15-ccc7-4425-a3c9-625be5579114)

## Open mail

Mails can be openned via panel *(by clicking the 👁)* or by opening `/preview/some.mail@test.com?k=your_key`

## API Access

The HTTP API is accessible by adding `?k=your_key` to the URL.

- **Retrieve bulk stored emails (JSON format):**
  ```
  GET /mails
  ```
  Pagination params:
  - `?limit`: The maximum amount of returned mails *(default: 20)*
  - `?offset`: The pagination offset *(default: 0)*


- **Retrieve a specific email (JSON format):**
  ```
  GET /mails/<email>
  ...
  Parameters:
  - ?limit: number of emails to return.
  - ?offset: number of emails to skip.
  ```


- **Delete a specific email:**
  ```
  DELETE /mails/<email>
  ```


- **Delete all stored emails:**
  ```
  DELETE /mails
  ```


## Notes
Port numbers under 1024 require root privileges. If you want to use a port number lower than 1024, you can use a reverse proxy like Nginx or Apache to forward the traffic to the Mail Sink server running on a higher port number.
