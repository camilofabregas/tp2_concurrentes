use std::error::Error;

use tokio::io::{AsyncBufReadExt, BufReader};

/// Prints an error message on stderr with the prefix [Error] in red.
/// `message` is a custom message, if "" is given, only e is printed
pub fn perror(message: &str, e: Option<Box<dyn Error>>) {
    if let Some(e) = e {
        let e = e.to_string();
        if !message.is_empty() {
            // custom + e
            eprintln!("\n\x1b[31m[Error]\x1b[0m {}: {}\n", message, e);
        } else {
            // e
            eprintln!("\n\x1b[31m[Error]\x1b[0m {}\n", e);
        }
    } else if !message.is_empty() {
        // custom
        eprintln!("\n\x1b[31m[Error]\x1b[0m {}\n", message);
    }
}

/// Awaits for user input until 'q' is pressed
pub async fn listen_user_input() {
    let mut async_stdin = BufReader::new(tokio::io::stdin());
    loop {
        let mut input = String::new();
        async_stdin.read_line(&mut input).await.unwrap();
        if input.trim() == "q" {
            break;
        }
    }
}
