use std::error::Error;
use std::net::SocketAddr;

use tokio::net::TcpStream;

use common::modules::constants::{GATEWAY_PORT, ROBOT_0_PORT, ROBOT_COUNT};

use crate::modules::utils::perror;

/// Exits process if an error occurs when establishing a connection
pub async fn connect_with_gateway() -> tokio::net::TcpStream {
    println!("\x1b[32m\nConnecting with gateway:\x1b[0m");
    let gateway_addr = SocketAddr::from(([127, 0, 0, 1], GATEWAY_PORT));

    match TcpStream::connect(gateway_addr).await {
        Ok(gateway_stream) => {
            println!("Connected to gateway at {}", gateway_addr);
            gateway_stream
        }
        Err(e) => {
            perror("Couldn't connect to gateway", Some(Box::new(e)));
            std::process::exit(1);
        }
    }
}

/// Attempts to connect with all robots. If all connections failed, returns an Error
pub async fn connect_with_robots() -> Result<Vec<tokio::net::TcpStream>, Box<dyn Error>> {
    let mut robots_addrs = Vec::new();
    for port in ROBOT_0_PORT as usize..(ROBOT_0_PORT as usize + ROBOT_COUNT) {
        robots_addrs.push(SocketAddr::from(([127, 0, 0, 1], port as u16)));
    }

    println!("\x1b[32m\nStarting connection with robots:\x1b[0m");

    let mut robots_streams = Vec::<TcpStream>::new();
    for addr in &robots_addrs {
        match TcpStream::connect(addr).await {
            Ok(stream) => {
                println!("Connected to robot at {}", addr);
                robots_streams.push(stream);
            }
            Err(_) => println!("Couldn't connect to robot at {}", addr),
        }
    }

    if robots_streams.is_empty() {
        return Err("No robots connected to screen.".into());
    }

    Ok(robots_streams)
}
