use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use crate::modules::utils::perror;

/// Panics if errors are found
pub fn parse_args() -> (u8, BufReader<File>) {
    let args: Vec<String> = std::env::args().collect();

    if args.len() != 3 {
        perror(
            "Error parsing arguments. Usage: cargo run <screen id> <orders.jsonl>",
            None,
        );
    }

    let screen_id: u8 = match args[1].parse() {
        // Reviso que el primer argumento sea un u8
        Ok(id) => id,
        Err(_) => {
            perror("Error parsing arguments. Screen ID must be an u8.", None);
            std::process::exit(1)
        }
    };

    // Reviso el path
    let orders_path = Path::new(std::env!("CARGO_MANIFEST_DIR"))
        .join("orders")
        .join(&args[2]);

    if !orders_path.exists() {
        perror("Error parsing arguments. Orders file doesn't exist", None);
        std::process::exit(1)
    } else if orders_path.extension().unwrap_or_default() != "jsonl" {
        perror(
            "Error parsing arguments. File must have .jsonl extension.",
            None,
        );
        std::process::exit(1)
    }

    let file = File::open(orders_path).unwrap();

    (screen_id, BufReader::new(file))
}
