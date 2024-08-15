use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader, Error},
    net::SocketAddr,
    sync::Arc,
};

use actix::fut::wrap_future;
use actix::prelude::*;

use serde_json::from_str;
use tokio::{
    io::{AsyncWriteExt, WriteHalf},
    net::TcpStream,
    sync::Mutex,
};

use common::modules::{
    disconnect::Disconnect,
    order_json::OrderJSON,
    order_prep::{OrderPrep, ORDER_FAILED, ORDER_SUCCESS, ROBOT_OCCUPIED},
    order_request::OrderRequest,
    payment_capture::PaymentCapture,
    payment_confirmation::PaymentConfirmation,
};

use crate::modules::utils::perror;

pub type WriteArcMutex = Arc<Mutex<WriteHalf<TcpStream>>>;

pub struct Screen {
    id: u8,
    reader: BufReader<File>,
    // (puerto donde escucho, donde escribo)
    gateway_write: (SocketAddr, WriteArcMutex),
    // (puerto destino robot), (puerto donde escucho, donde escribo)
    robots_write: HashMap<SocketAddr, (SocketAddr, WriteArcMutex)>,
    current_order: Option<OrderPrep>,
    order_in_process: bool,
    finished_orders: Vec<usize>, // Contiene ids de ordenes finalizadas
}

impl Screen {
    pub fn new(
        id: u8,
        reader: BufReader<File>,
        gateway_write: (SocketAddr, WriteArcMutex),
        robots_write: HashMap<SocketAddr, (SocketAddr, WriteArcMutex)>,
    ) -> Self {
        Screen {
            id,
            reader,
            gateway_write,
            robots_write,
            current_order: None,
            order_in_process: false,
            finished_orders: Vec::new(),
        }
    }

    /// Sends the message through the provided WriteArcMutex
    fn send_message(&mut self, ctx: &mut Context<Self>, msg: String, stream_arc: WriteArcMutex) {
        wrap_future::<_, Self>(async move {
            // Podrías hacer un logger y tener prints de debug
            // println!("Sending: {:?}", msg);
            stream_arc
                .lock()
                .await
                .write_all((msg + "\n").as_bytes())
                .await
                .expect("Should have sent message");
        })
        .spawn(ctx);
    }

    /// Sends the request for the current order to all connected robots.
    fn broadcast_request(&mut self, ctx: &mut Context<Self>) {
        let order_id = self.current_order.clone().unwrap().id;
        println!("Broadcasting order request {}.", order_id);
        // Para evitar error de manejo de self al hacer self.robots_write.iter()
        let robots_write = self.robots_write.clone();

        for (_, (ip, write)) in robots_write.iter() {
            let request = OrderRequest::new(ip.to_string(), order_id);
            let msg = serde_json::to_string(&request).unwrap();
            self.send_message(ctx, msg, write.clone());
        }
    }

    /// Sends a PaymentConfirmation message to the gateway
    fn confirm_payment(&mut self, ctx: &mut Context<Self>, order_data: OrderPrep) {
        // Confirmar el pago con el gateway
        let local_ip = self.gateway_write.0.to_string();
        let confirmation = PaymentConfirmation::new(local_ip, self.id.to_string(), order_data);
        let msg = serde_json::to_string(&confirmation).unwrap();
        let stream_arc: WriteArcMutex = self.gateway_write.1.clone();

        self.send_message(ctx, msg, stream_arc);
    }

    /// Starts a 30 second async timer that sends a ReBroadCastOrder message to the screen
    /// when finished
    fn start_order_timer(&mut self, ctx: &mut Context<Self>) {
        let screen_address: Addr<Screen> = ctx.address();
        let id = self.current_order.clone().unwrap().id;

        wrap_future::<_, Self>(async move {
            println!("Starting timer");
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
            let _ = screen_address.send(ReBroadcastOrder { id }).await;
        })
        .spawn(ctx);
    }

    // MESSAGE HANDLERS ----------------------------------------------------------------------------

    /// Stores the Order on current_order and sends a PaymentCapture message to the Gateway
    fn handle_new_order(&mut self, ctx: &mut Context<Self>, line: String) {
        match from_str::<OrderJSON>(&line) {
            Ok(order_json) => {
                println!("\x1b[1m\x1b[4mReceived order\x1b[0m: {:?}", order_json);
                // Guardo el pedido
                self.current_order = Some(OrderPrep::from(order_json));

                // Capturo el pago
                let payment = PaymentCapture::new(
                    self.gateway_write.0.to_string(),
                    self.id.to_string(),
                    true,
                );
                let msg = serde_json::to_string(&payment).unwrap();
                let stream_arc: WriteArcMutex = self.gateway_write.1.clone();
                self.send_message(ctx, msg, stream_arc)
            }
            Err(e) => eprintln!("Failed to deserialize order: {}", e),
        }
    }

    /// If the capture was successful, it broadcasts the order request. If not, it cancels the
    /// current order and receives the next one.
    fn handle_payment_capture(&mut self, ctx: &mut Context<Self>, msg: String) {
        match from_str::<PaymentCapture>(&msg) {
            Ok(capture) => {
                if capture.valid {
                    self.broadcast_request(ctx);
                } else {
                    println!(
                        "\x1b[31m✘\x1b[0m Payment couldn't be captured, order is cancelled.\n"
                    );
                    ctx.address()
                        .try_send(ReceiveOrder())
                        .expect("Couldn't send 'ReceiveOrder' at payment capture.");
                }
            }

            Err(e) => perror("", Some(Box::new(e))),
        }
    }

    /// If the order was not being processed by another robot, send an OrderPrep message to the
    /// robot to begin preparation. Else, the message is ignored.
    fn handle_order_request(&mut self, ctx: &mut Context<Self>, msg: String) {
        let request = match from_str::<OrderRequest>(&msg) {
            Ok(request) => request,
            Err(e) => {
                perror("", Some(Box::new(e)));
                return;
            }
        };

        let robot_addr: SocketAddr = request
            .ip
            .parse()
            .expect("Couldn't parse SocketAddr at handle_order_request.");
        let mut order = self.current_order.clone().unwrap();
        // Si ya hay algún robot procesando el pedido, ignoro el mensaje
        if !self.order_in_process && self.current_order.is_some() {
            if let Some((local_ip, write)) = self.robots_write.get(&robot_addr) {
                order.ip = local_ip.to_string();
                let msg = serde_json::to_string(&order).unwrap();

                self.send_message(ctx, msg, write.clone());

                println!("Sent order to robot at {}.\nWaiting...", robot_addr);
                self.order_in_process = true;
                self.start_order_timer(ctx);
            }
        } else {
            println!(
                "\x1b[34m[DEBUG]\x1b[0m Ignored {}. Order {} already taken.",
                robot_addr, order.id
            );
        }
    }

    /// If the order was successful, it stores the order as finished, sends a payment confirmation
    /// to the gateway, and continues to the next order.
    /// If the order failed, finished the order, but do not send a payment confirmation and
    /// receive a new order.
    /// If the robot was occupied, do nothing (this is handled by the timer)
    fn handle_order_result(&mut self, ctx: &mut Context<Self>, msg: String) {
        let result = match from_str::<OrderPrep>(&msg) {
            Ok(request) => request,
            Err(e) => {
                perror("", Some(Box::new(e)));
                return;
            }
        };

        if result.fail_flag as u8 == ORDER_SUCCESS {
            println!(
                "\x1b[32m✔\x1b[0m Order {} completed, sending payment confirmation to gateway.\n",
                result.id
            );
            self.finished_orders.push(result.id);
            self.confirm_payment(ctx, result);
        } else if result.fail_flag as u8 == ORDER_FAILED {
            println!("\x1b[31m✘\x1b[0m Not enough ice cream, order is cancelled.\n");
            self.finished_orders.push(result.id);
        } else if result.fail_flag as u8 == ROBOT_OCCUPIED {
            println!("Received ROBOT_OCCUPIED.");
            self.order_in_process = false;
            // self.broadcast_request(ctx);
            return; // Espero al timer
        }
        // Dejo todo listo para el siguiente pedido
        self.order_in_process = false;
        self.current_order = None;
        // Arranco el siguiente pedido
        ctx.address().do_send(ReceiveOrder());
    }
}

impl Actor for Screen {
    type Context = Context<Self>;
}

// Messages --------------------------------------------------------------------
impl StreamHandler<Result<String, Error>> for Screen {
    fn handle(&mut self, msg: Result<String, Error>, ctx: &mut Self::Context) {
        match msg {
            Ok(msg) => {
                if msg.contains("\"PaymentCapture\"") {
                    self.handle_payment_capture(ctx, msg);
                } else if msg.contains("\"OrderRequest\"") {
                    self.handle_order_request(ctx, msg);
                } else if msg.contains("\"OrderPrep\"") {
                    self.handle_order_result(ctx, msg);
                } else {
                    perror(
                        format!("Received unknown message at StreamHandler: {}", msg).as_str(),
                        None,
                    )
                }
            }

            Err(e) => perror(
                "Received invalid message at StreamHandler",
                Some(Box::new(e)),
            ),
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ReceiveOrder();

/// Makes the Screen read a line from the orders file and process it.
impl Handler<ReceiveOrder> for Screen {
    type Result = ();

    fn handle(&mut self, _msg: ReceiveOrder, ctx: &mut Context<Self>) -> Self::Result {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => println!("No more orders left. Waiting for shutdown."),

            Ok(_) => self.handle_new_order(ctx, line),

            Err(e) => eprintln!("Error reading order : {}", e),
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Shutdown();

/// Finishes execution in an ordered manner, sending a Disconnect message to the gateway.
impl Handler<Shutdown> for Screen {
    type Result = ();

    fn handle(&mut self, _msg: Shutdown, ctx: &mut Context<Self>) -> Self::Result {
        // Envío disconnect al gateway
        println!("Disconnecting from gateway.");
        let local_ip = self.gateway_write.0.to_string();
        let disconnect = Disconnect::new(local_ip, self.id.to_string());
        let msg = serde_json::to_string(&disconnect).unwrap();
        let stream_arc = self.gateway_write.1.clone();
        self.send_message(ctx, msg, stream_arc);
        // TODO: envío disconnect a los robots (todavía no handlean el mensaje disconnect)
        // ctx.stop();
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ReBroadcastOrder {
    id: usize,
}

// Tengo que hacer esta movida para tener acceso a self cuando se dispara el timeout
impl Handler<ReBroadcastOrder> for Screen {
    type Result = ();

    fn handle(&mut self, msg: ReBroadcastOrder, ctx: &mut Context<Self>) -> Self::Result {
        if !self.finished_orders.contains(&msg.id) {
            println!("\x1b[31m[Timeout]\x1b[0m Re-broadcasting order {}.", msg.id);
            self.order_in_process = false;
            self.broadcast_request(ctx);
        } else {
            println!("\x1b[34m[DEBUG]\x1b[0m Iba a hacer un re-broadcast de la orden {} pero se completó el pedido antes.", msg.id);
        }
    }
}
