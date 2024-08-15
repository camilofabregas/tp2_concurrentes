use std::{collections::HashMap, fs::File, net::SocketAddr, sync::Arc};

use actix::prelude::*;
use tokio::{
    io::{split, AsyncBufReadExt, BufReader},
    net::TcpStream,
    sync::Mutex,
};
use tokio_stream::wrappers::LinesStream;

use crate::modules::screen::{Screen, WriteArcMutex};

/// Creates the Screen Actix Actor
pub fn create_screen(
    id: u8,
    reader: std::io::BufReader<File>,
    gateway_stream: TcpStream,
    robot_streams: Vec<TcpStream>,
) -> Addr<Screen> {
    Screen::create(|ctx| {
        let gateway_write: (SocketAddr, WriteArcMutex) = set_gateway_stream(gateway_stream, ctx);

        let robots_write: HashMap<SocketAddr, (SocketAddr, WriteArcMutex)> =
            set_robots_streams(robot_streams, ctx);

        Screen::new(id, reader, gateway_write, robots_write)
    })
}

/// Retorna (puerto donde escucho, donde escribo)
fn set_gateway_stream(stream: TcpStream, ctx: &mut Context<Screen>) -> (SocketAddr, WriteArcMutex) {
    let ip = stream.local_addr().unwrap();
    let (read_half, write_half) = split(stream);
    let stream = LinesStream::new(BufReader::new(read_half).lines());
    Screen::add_stream(stream, ctx);

    (ip, Arc::new(Mutex::new(write_half)))
}

/// Retorna (puerto destino robot), (puerto donde escucho, donde escribo)
fn set_robots_streams(
    streams: Vec<TcpStream>,
    ctx: &mut Context<Screen>,
) -> HashMap<SocketAddr, (SocketAddr, WriteArcMutex)> {
    let mut robots_write: HashMap<SocketAddr, (SocketAddr, WriteArcMutex)> = HashMap::new();
    for stream in streams {
        let local_ip = stream.local_addr().unwrap();
        let robot_ip = stream.peer_addr().unwrap();

        let (read_half, write_half) = split(stream);
        let stream = LinesStream::new(BufReader::new(read_half).lines());
        Screen::add_stream(stream, ctx);
        robots_write.insert(robot_ip, (local_ip, Arc::new(Mutex::new(write_half))));
    }

    robots_write
}
