use std::collections::HashMap;
use std::{env, sync::Arc};

use tokio::io::{split, AsyncBufReadExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_stream::wrappers::LinesStream;

use actix::{Actor, StreamHandler};

mod flavour;
mod robot;
use robot::{connect_robots, Robot, RobotReconnect, RobotStart, Shutdown, ROBOT_IP_PREFIX};

#[actix_rt::main]
async fn main() {
    let id: usize = env::args()
        .nth(1)
        .expect("[ERROR] Missing id parameter")
        .parse()
        .expect("[ERROR] id must be a number");

    let (previous_robot, next_robot, screens) = connect_robots(id).await;

    let robot = Robot::create(|ctx| {
        let addr_previous = previous_robot.1;

        let local_ip_previous = previous_robot
            .0
            .local_addr()
            .expect("[ERROR] Couldn't get local IP address");
        let (read_previous, write_half_previous) = split(previous_robot.0);
        Robot::add_stream(LinesStream::new(BufReader::new(read_previous).lines()), ctx);
        let write_previous = Arc::new(Mutex::new(write_half_previous));

        let local_ip_next = next_robot
            .local_addr()
            .expect("[ERROR] Couldn't get local IP address");
        let (read_next, write_half_next) = split(next_robot);
        Robot::add_stream(LinesStream::new(BufReader::new(read_next).lines()), ctx);
        let write_next = Arc::new(Mutex::new(write_half_next));

        let mut screen_connections = HashMap::new();
        for connection in screens {
            let local_ip = connection
                .1
                .local_addr()
                .expect("[ERROR] Couldn't get local IP address");
            let (read, write_half) = split(connection.1);
            Robot::add_stream(LinesStream::new(BufReader::new(read).lines()), ctx);
            let write = Arc::new(Mutex::new(write_half));
            screen_connections.insert(connection.0, (write, local_ip));
        }

        let need_flavours = HashMap::from([
            ("Vainilla".to_string(), 0.0),
            ("Dulce de leche".to_string(), 0.0),
            ("Tramontana".to_string(), 0.0),
        ]);

        let ack_flavours = HashMap::from([
            ("Vainilla".to_string(), 0),
            ("Dulce de leche".to_string(), 0),
            ("Tramontana".to_string(), 0),
        ]);

        let current_order = None;

        Robot::new(
            id,
            (addr_previous, (write_previous, local_ip_previous)),
            (write_next, local_ip_next),
            screen_connections,
            need_flavours,
            ack_flavours,
            current_order,
        )
    });

    if id == 0 {
        match robot.send(RobotStart()).await {
            Ok(_) => println!("[ROBOT {}] Starting ring, sending initial tokens", id),
            Err(_) => println!("[ROBOT {}] Error while sending the initial tokens", id),
        }
    }

    let mut async_stdin = BufReader::new(tokio::io::stdin()).lines();
    let listener = TcpListener::bind(ROBOT_IP_PREFIX.to_string() + &*id.to_string())
        .await
        .unwrap();

    loop {
        tokio::select! {
            Ok(Some(input)) = async_stdin.next_line() => {
                if input.trim() == "q" {
                    break;
                }
            }
            Ok((stream, addr)) = listener.accept() => {
                println!("[ROBOT {}] New Robot connected with address {}", id, addr);
                let local_ip = stream.local_addr().expect("[ERROR] Couldn't get local IP address");
                let (read, write_half) = split(stream);
                let reader = BufReader::new(read).lines();
                match robot.send(RobotReconnect((addr, write_half, local_ip, reader))).await {
                    Ok(_) => println!("[ROBOT {}] Ring reconnected and closed", id),
                    Err(_) => println!("[ROBOT {}] Error while reconnecting ring", id),
                }
            }
        }
    }

    // Cierre ordenado
    let _ = robot.send(Shutdown()).await;
    // Para dar tiempo a que se envien los Disconnect
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
}
