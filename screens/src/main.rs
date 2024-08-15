use actix::prelude::*;
use tokio::io::AsyncWriteExt; // For shutdown()

mod modules;
use modules::{
    connections::{connect_with_gateway, connect_with_robots},
    init::create_screen,
    parser::parse_args,
    screen::{ReceiveOrder, Screen, Shutdown},
    utils::{listen_user_input, perror},
};

#[actix_rt::main]
async fn main() {
    let (screen_id, reader) = parse_args();

    let mut gateway_stream = connect_with_gateway().await;

    let robot_streams = match connect_with_robots().await {
        Ok(robot_streams) => robot_streams,
        Err(e) => {
            gateway_stream
                .shutdown()
                .await
                .expect("Unable to shutdown gateway_stream");
            perror("", Some(e));
            std::process::exit(1)
        }
    };

    // Creo el actor Screen
    let screen: Addr<Screen> = create_screen(screen_id, reader, gateway_stream, robot_streams);

    println!(
        "\x1b[32m\nScreen {} is ready to receive orders.\x1b[0m",
        screen_id
    );
    println!("Presiona 'q' para salir: \n");

    // Arranco con la primer orden
    let _ = screen.send(ReceiveOrder()).await;

    listen_user_input().await;

    // Cierre ordenado
    let _ = screen.send(Shutdown()).await;
    // Para dar tiempo a que se envien los Disconnect
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
}
