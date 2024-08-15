mod gateway;
use actix::System;
use actix::{Actor, StreamHandler};
use common::modules::constants::SCREEN_COUNT;
use common::modules::disconnect;
use common::modules::payment_capture;
use common::modules::payment_confirmation;
use gateway::Gateway;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{split, AsyncBufReadExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_stream::wrappers::LinesStream;

const GATEWAY_ID: &str = "GATEWAY";
const GATEWAY_IP: &str = "127.0.0.1:20000";

/// Start the Gateway server, and accept the connections from all the Screen instances.
/// After that, create the Gateway actor with the Write half of the stream of each Screen.
fn main() {
    let system = System::new();

    system.block_on(async {
        let gateway_server = TcpListener::bind(GATEWAY_IP)
            .await
            .expect("[ERROR] Couldn't start the Gateway server");
        println!("[{}] Awaiting for incoming connections", GATEWAY_ID);

        let mut connections = HashMap::new();
        for _ in 0..SCREEN_COUNT {
            let (stream, addr) = gateway_server
                .accept()
                .await
                .expect("[ERROR] Couldn't establish a connection with the client");
            connections.insert(addr.to_string(), stream);
        }

        Gateway::create(|ctx| {
            let mut screen_connections = HashMap::new();
            for connection in connections {
                let (read, write_half) = split(connection.1);
                Gateway::add_stream(LinesStream::new(BufReader::new(read).lines()), ctx);
                let write = Arc::new(Mutex::new(write_half));
                screen_connections.insert(connection.0, write);
            }
            Gateway::new(screen_connections)
        });
    });

    system
        .run()
        .expect("[ERROR] Couldn't run Gateway with Actix System");
}
