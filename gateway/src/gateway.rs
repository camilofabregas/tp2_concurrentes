use crate::disconnect::Disconnect;
use crate::payment_capture::PaymentCapture;
use crate::payment_confirmation::PaymentConfirmation;
use crate::{GATEWAY_ID, GATEWAY_IP};
use actix::fut::wrap_future;
use actix::{Actor, ActorContext, Context, ContextFutureSpawner, StreamHandler};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::io::WriteHalf;
use tokio::net::TcpStream;
use tokio::sync::Mutex;

/// Gateway has a HashMap of connections, with the IP and the WriteHalf of the stream of each Screen.
pub struct Gateway {
    connections: HashMap<String, Arc<Mutex<WriteHalf<TcpStream>>>>,
}

impl Gateway {
    /// Create a new Gateway instance.
    pub fn new(_connections: HashMap<String, Arc<Mutex<WriteHalf<TcpStream>>>>) -> Self {
        Gateway {
            connections: _connections,
        }
    }

    /// Send a message to a certain Screen.
    pub fn write_message(
        ctx: &mut Context<Gateway>,
        message: String,
        destination: Arc<Mutex<WriteHalf<TcpStream>>>,
    ) {
        wrap_future::<_, Self>(async move {
            destination
                .lock()
                .await
                .write_all((message + "\n").as_bytes())
                .await
                .expect("[ERROR] Couldn't send message to the client")
        })
        .spawn(ctx);
    }
}

impl Actor for Gateway {
    type Context = Context<Self>;
}

/// Read a message from the Gateway's FIFO and act accordingly:
/// If it's a PaymentCapture message, capture the payment and answer to the Screen.
/// If it's a PaymentConfirmation message, log it to disk.
/// If it's a Disconnect message, remove the sender from the connections HashMap.
impl StreamHandler<Result<String, std::io::Error>> for Gateway {
    fn handle(&mut self, read: Result<String, std::io::Error>, ctx: &mut Self::Context) {
        if let Ok(line) = read {
            if line.contains("\"PaymentCapture\"") {
                let mut capture: PaymentCapture =
                    serde_json::from_str(&line).expect("Couldn't deserialize PaymentCapture");
                let client_ip = capture.ip.clone();
                let client_id = capture.id.clone();
                println!("[{}] wants to capture a payment", capture.id);

                capture = PaymentCapture::capture_payment(capture, GATEWAY_ID, GATEWAY_IP);
                let capture_str =
                    serde_json::to_string(&capture).expect("Couldn't deserialize PaymentCapture");
                Gateway::write_message(ctx, capture_str, self.connections[&client_ip].clone());

                if capture.valid {
                    println!(
                        "[{}] the payment from {} was captured succesfully ",
                        GATEWAY_ID, client_id
                    );
                } else {
                    println!(
                        "[{}] the payment from {} couldn't be captured",
                        GATEWAY_ID, client_id
                    );
                }
            } else if line.contains("\"PaymentConfirmation\"") {
                let confirmation: PaymentConfirmation =
                    serde_json::from_str(&line).expect("Couldn't deserialize PaymentConfirmation");
                println!("[{}] wants to confirm a payment", confirmation.id);
                if let Ok(()) = PaymentConfirmation::confirm_payment(&confirmation) {
                    println!(
                        "[{}] the payment from {} was confirmed",
                        GATEWAY_ID, confirmation.id
                    );
                } else {
                    println!(
                        "[ERROR] There was an error while trying to confirm the payment from {}",
                        confirmation.id
                    );
                }
            } else if line.contains("\"Disconnect\"") {
                let disconnect: Disconnect =
                    serde_json::from_str(&line).expect("Couldn't deserialize Disconnect");
                self.connections.remove(&disconnect.ip);
                println!("[EXIT] {} has disconnected", disconnect.id);
            } else {
                println!(
                    "[ERROR] An unknown message with code {} was received",
                    &line[5..]
                );
            }
        } else {
            println!("[ERROR] Failed to read line {:?}", read);
        }
    }

    /// Stop the context if all the Screens have disconnected.
    fn finished(&mut self, ctx: &mut Self::Context) {
        if self.connections.is_empty() {
            println!("[EXIT] All screens are disconnected. Stopping context...");
            ctx.stop();
        }
    }
}
