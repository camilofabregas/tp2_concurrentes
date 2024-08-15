use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{net::SocketAddr, sync::Arc, thread, time::Duration};

use actix::fut::wrap_future;
use actix::{
    Actor, AsyncContext, Context, ContextFutureSpawner, Handler, Message, StreamHandler, WrapFuture,
};
use common::modules::disconnect::Disconnect;
use tokio::io::{split, AsyncBufReadExt, AsyncWriteExt, BufReader, Lines, ReadHalf, WriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::time::sleep;
use tokio_stream::wrappers::LinesStream;

use crate::flavour::Flavour;

use common::modules::constants::ROBOT_COUNT;
use common::modules::constants::SCREEN_COUNT;
use common::modules::order_prep::OrderPrep;
use common::modules::order_request::OrderRequest;

const FLAVOUR_TIMEOUT: u128 = 60000;
pub const ROBOT_IP_PREFIX: &str = "127.0.0.1:3000";

type Connection = (Arc<Mutex<WriteHalf<TcpStream>>>, SocketAddr);

/// Robot Actor. Receives and sends Flavours to other Robots. Processes Orders from Screens.
pub struct Robot {
    id: usize,
    addr_previous: SocketAddr,
    previous_robot: Connection,
    next_robot: Connection,
    screens: HashMap<String, Connection>,
    need_flavours: HashMap<String, f64>,
    ack_flavours: HashMap<String, u128>,
    current_order: Option<OrderPrep>,
}

impl Robot {
    /// Robot constructor.
    pub fn new(
        id: usize,
        previous: (SocketAddr, Connection),
        next_robot: Connection,
        screens: HashMap<String, Connection>,
        need_flavours: HashMap<String, f64>,
        ack_flavours: HashMap<String, u128>,
        current_order: Option<OrderPrep>,
    ) -> Self {
        let addr_previous = previous.0;
        let previous_robot = previous.1;
        Robot {
            id,
            addr_previous,
            previous_robot,
            next_robot,
            screens,
            need_flavours,
            ack_flavours,
            current_order,
        }
    }

    /// Handles an incoming Flavour. If it has an Order than needs it, it consumes the required amount, if possible.
    /// If the Order is complete or cancelled, it sends a message to the Screen.
    fn process_flavour(&mut self, ctx: &mut Context<Self>, message_str: String) {
        let mut flavour: Flavour =
            serde_json::from_str(&message_str).expect("[ERROR] Couldn't deserialize Flavour");
        println!(
            "[ROBOT {}] Flavour {} received, amount: {}",
            self.id, flavour.name, flavour.amount
        );

        if self.need_flavours[&flavour.name] > 0.0 && self.current_order.is_some() {
            if flavour.amount >= self.need_flavours[&flavour.name] {
                println!(
                    "[ROBOT {}] Consuming {} of Flavour {}",
                    self.id, self.need_flavours[&flavour.name], flavour.name
                );
                // Sleep según la cantidad de helado utilizada
                let _sleep = Box::pin(
                    sleep(Duration::from_secs(2 * flavour.amount as u64)).into_actor(self),
                );
                flavour.amount -= self.need_flavours[&flavour.name];
                self.need_flavours.insert(flavour.name.to_string(), 0.0);
                if self.need_flavours.values().all(|&x| x == 0.0) {
                    // Terminé Order con éxito
                    println!("[ROBOT {:?}] Order Prep completed", self.id);
                    let fail_flag = 0; // Significa que la orden se preparó correctamente
                    self.send_order_prep(ctx, fail_flag);
                }
            } else {
                // Si no hay cantidad suficiente de helado, cancelar Order
                println!(
                    "[ROBOT {:?}] Order Prep cancelled, not enough ice cream",
                    self.id
                );
                let fail_flag = 1; // Significa que no hay cantidad suficiente de helado
                self.send_order_prep(ctx, fail_flag);
            }
        }
        let flavour_new_str =
            serde_json::to_string(&flavour).expect("[ERROR] Couldn't serialize Flavour") + "\n";
        println!("[ROBOT {}] Sending {}", self.id, &flavour.name);
        self.send_message(ctx, flavour_new_str, self.next_robot.0.clone());

        // self.ack_flavours.insert(flavour.name.to_string(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis());

        // let ack = String::from("ACKToken".to_string() + &flavour.name + "\n");
        // self.send_message(ctx, ack, self.write_previous.clone());
    }

    /// Handles an incoming Flavour ACK.
    fn process_ack(&mut self, message_str: String) {
        let flavour = message_str[8..].to_string();
        println!("[ROBOT {}] ACK of flavour {}", self.id, flavour);
        self.ack_flavours.insert(flavour.clone(), 0);
    }

    /// Handles an Order Request from a Screen.
    /// If it has no current Order, it accepts it and notifies the Screen.
    fn process_order_request(&mut self, ctx: &mut Context<Self>, message_str: String) {
        println!("[ROBOT {:?}] Order Request from Screen", self.id);
        if self.current_order.is_none() {
            // Aceptar pedido y responder a la pantalla
            println!("[ROBOT {:?}] Order Request accepted", self.id);
            let mut order_request: OrderRequest = serde_json::from_str(&message_str)
                .expect("[ERROR] Couldn't deserialize OrderRequest");
            let screen_addr = order_request.ip;
            let screen_stream = self.screens[&screen_addr].clone();
            order_request.id = self.id;
            order_request.ip = ROBOT_IP_PREFIX.to_string() + &self.id.to_string();
            let order_request_response_str = serde_json::to_string(&order_request)
                .expect("[ERROR] Couldn't serialize OrderRequest")
                + "\n";
            self.send_message(ctx, order_request_response_str, screen_stream.0);
        } else {
            // Rechazar pedido, ya tengo uno, no respondo a la pantalla
            // Podria reenviar el mensaje OrderRequest con un flag que indique que el Robot esta ocupado
            println!(
                "[ROBOT {:?}] Order Request ignored, already have an Order",
                self.id
            );
        }
    }

    /// Handles an Order Prep from a Screen.
    /// If doesn't have a current Order, it accepts it and waits for the needed Flavours.
    /// If it has a current Order, it is rejected and notifies the Screen.
    fn process_order_prep(&mut self, ctx: &mut Context<Self>, message_str: String) {
        println!("[ROBOT {}] Order Prep from Screen", self.id);
        let mut order: OrderPrep =
            serde_json::from_str(&message_str).expect("[ERROR] Couldn't deserialize OrderPrep");
        if self.current_order.is_some() {
            // Si ya tome una order (tengo Some(Order)), tengo que rechazarla
            println!(
                "[ROBOT {:?}] Order Prep rejected, already have an Order",
                self.id
            );
            order.fail_flag = 2; // Significa que ya tengo una Order y no puedo tomarla, que la Pantalla intente con otro Robot.
            let screen_addr = order.ip.to_string();
            let screen_stream = self.screens[&screen_addr].clone();
            order.ip = ROBOT_IP_PREFIX.to_string() + &self.id.to_string();
            let order_prep_str =
                serde_json::to_string(&order).expect("[ERROR] Couldn't serialize OrderPrep") + "\n";
            self.send_message(ctx, order_prep_str, screen_stream.0);
        } else {
            println!("[ROBOT {}] Order Prep accepted, preparing Order", self.id);
            let size_per_flavour = order.size as f64 / order.flavours.len() as f64;
            for flavour in &order.flavours {
                self.need_flavours
                    .insert(flavour.to_string(), size_per_flavour);
            }
            self.current_order = Some(order);
        }
    }

    /// Sends a message with the successful or cancelled Order Prep to the Screen.
    fn send_order_prep(&mut self, ctx: &mut Context<Self>, fail_flag: usize) {
        let mut order_prep = self
            .current_order
            .take()
            .expect("Will always have an Order at this point");
        let screen_addr = order_prep.ip.to_string();
        let screen_stream = self.screens[&screen_addr].clone();
        order_prep.ip = ROBOT_IP_PREFIX.to_string() + &self.id.to_string();
        order_prep.fail_flag = fail_flag;
        let order_prep_str = serde_json::to_string(&order_prep)
            .expect("[ERROR] Couldn't serialize OrderPrep")
            + "\n";
        self.send_message(ctx, order_prep_str, screen_stream.0);
    }

    /// Sends a message to the desired destination.
    fn send_message(
        &mut self,
        ctx: &mut Context<Self>,
        msg: String,
        dest: Arc<Mutex<WriteHalf<TcpStream>>>,
    ) {
        wrap_future::<_, Self>(async move {
            dest.lock()
                .await
                .write_all(msg.as_bytes())
                .await
                .unwrap_or_else(|_| {
                    println!("[ERROR] Couldn't send the following message:");
                    println!("{}", msg);
                });
        })
        .spawn(ctx);
    }

    /// Handles a Disconnect message.
    /// If the next Robot disconnected, it tries to connect to the next one available.
    fn process_disconnect(&mut self, ctx: &mut Context<Self>, message_str: String) {
        let disconnect: Disconnect =
            serde_json::from_str(&message_str).expect("[ERROR] Couldn't deserialize Disconnect");
        if disconnect.ip == self.addr_previous.to_string() {
            println!("[ROBOT {}] The previous Robot is down", self.id);
            // self.skip_dead_robot(ctx, "previous".to_string());
            println!("[ROBOT {}] Wating for reconnection", self.id);
        } else {
            println!("[ROBOT {}] The next Robot is down", self.id);
            self.skip_dead_robot(ctx);
        }
        //self.screens.remove(&disconnect.ip);
        //println!("[EXIT] {} has disconnected", disconnect.id);
    }

    /// Connects to the next available Robot and updates its connections.
    fn skip_dead_robot(&mut self, ctx: &mut Context<Self>) {
        println!("[ROBOT {}] Connecting to the next available Robot", self.id);

        let id_robot = if self.id == ROBOT_COUNT - 1 || self.id == ROBOT_COUNT - 2 {
            (self.id + 2) - ROBOT_COUNT
        } else {
            self.id + 2
        };

        let new_robot_std =
            std::net::TcpStream::connect(ROBOT_IP_PREFIX.to_string() + &*(id_robot).to_string())
                .unwrap();
        let new_robot = TcpStream::from_std(new_robot_std).unwrap();
        let local_ip_new = new_robot
            .local_addr()
            .expect("[ERROR] Couldn't get local IP address");
        let (read_new, write_half_new) = split(new_robot);
        ctx.add_stream(LinesStream::new(BufReader::new(read_new).lines()));
        let write_new = Arc::new(Mutex::new(write_half_new));
        self.next_robot = (write_new, local_ip_new);
        println!("[ROBOT {}] Connected to Robot {}", self.id, id_robot);
    }
}

impl Actor for Robot {
    type Context = Context<Self>;
}

/// Shutdown message for Robots.
#[derive(Message)]
#[rtype(result = "()")]
pub struct Shutdown();

impl Handler<Shutdown> for Robot {
    type Result = ();

    /// Handles the Shutdown message.
    /// Notifies the previous and next Robots that it is disconnecting with Disconnect messages.
    fn handle(&mut self, _msg: Shutdown, ctx: &mut Context<Self>) -> Self::Result {
        println!("[ROBOT {}] Shutting down, disconnecting", self.id);

        println!("[ROBOT {}] Sending Disconnect to previous Robot", self.id);
        let disconnect_previous =
            Disconnect::new(self.previous_robot.1.to_string(), self.id.to_string());
        let msg_previous = serde_json::to_string(&disconnect_previous)
            .expect("[ERROR] Couldn't serialize Disconnect");
        self.send_message(ctx, msg_previous, self.previous_robot.0.clone());

        println!("[ROBOT {}] Sending Disconnect to next Robot", self.id);
        let disconnect_next = Disconnect::new(self.next_robot.1.to_string(), self.id.to_string());
        let msg_next =
            serde_json::to_string(&disconnect_next).expect("[ERROR] Couldn't serialize Disconnect");
        self.send_message(ctx, msg_next, self.next_robot.0.clone());

        // TODO: envío disconnect a las Screens
        /*for screen in self.screens.values() {
            let disconnect_screen = Disconnect::new(screen.1.to_string(), self.id.to_string());
            let msg_screen = serde_json::to_string(&disconnect_screen).expect("[ERROR] Couldn't serialize Disconnect");
            self.send_message(ctx, msg_screen, screen.0.clone());
        }*/
    }
}

/// RobotReconnect message for Robots.
#[derive(Message)]
#[rtype(result = "()")]
pub struct RobotReconnect(
    pub  (
        SocketAddr,
        WriteHalf<TcpStream>,
        SocketAddr,
        Lines<BufReader<ReadHalf<TcpStream>>>,
    ),
);

impl Handler<RobotReconnect> for Robot {
    type Result = ();

    /// Handles the RobotReconnect message.
    /// When a new previous Robot connects, it adds the new connection to the Robot.
    fn handle(&mut self, msg: RobotReconnect, ctx: &mut Context<Self>) {
        println!(
            "[ROBOT {}] Adding the previous Robot to my connections",
            self.id
        );

        ctx.add_stream(LinesStream::new(msg.0 .3));
        let write = Arc::new(Mutex::new(msg.0 .1));
        self.addr_previous = msg.0 .0;
        self.previous_robot = (write, msg.0 .2);
    }
}

/// RobotStart message for Robots.
#[derive(Message)]
#[rtype(result = "()")]
pub struct RobotStart();

impl Handler<RobotStart> for Robot {
    type Result = ();

    /// Handles the RobotStart message.
    /// Creates the Flavours and sends them to the next robot to initialize the ring.
    fn handle(&mut self, _msg: RobotStart, ctx: &mut Context<Self>) {
        println!("[ROBOT {}] Building flavours...", self.id);

        let vainilla = Flavour::new("Vainilla".to_string(), 10.0);
        let vainilla_str =
            serde_json::to_string(&vainilla).expect("[ERROR] Couldn't serialize Vainilla");
        println!("{:?}", vainilla_str);
        self.send_message(ctx, vainilla_str + "\n", self.next_robot.0.clone());

        let ddl = Flavour::new("Dulce de leche".to_string(), 10.0);
        let ddl_str = serde_json::to_string(&ddl).expect("[ERROR] Couldn't serialize Vainilla");
        println!("{:?}", ddl_str);
        self.send_message(ctx, ddl_str + "\n", self.next_robot.0.clone());

        let tramontana = Flavour::new("Tramontana".to_string(), 10.0);
        let tramontana_str =
            serde_json::to_string(&tramontana).expect("[ERROR] Couldn't serialize Vainilla");
        println!("{:?}", tramontana_str);
        self.send_message(ctx, tramontana_str + "\n", self.next_robot.0.clone());
    }
}

impl StreamHandler<Result<String, std::io::Error>> for Robot {
    /// Handles socket messages from other Robots and Screens.
    /// Matches the message type to its corresponding processing function.
    fn handle(&mut self, read: Result<String, std::io::Error>, ctx: &mut Self::Context) {
        if let Ok(message_str) = read {
            println!("\n[ROBOT {}] Received message", self.id);

            // Sleep para que la ejecución sea legible
            thread::sleep(Duration::from_secs(2));

            for flavour in &self.ack_flavours {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis();
                if *flavour.1 != 0 && now - flavour.1 > FLAVOUR_TIMEOUT {
                    println!(
                        "[ROBOT {}] The following flavour was lost: {}",
                        self.id, flavour.0
                    );
                    // Chequear si los otros sabores tienen timestamp menor.
                    // Si tienen timestamp mayor, no se enviaron al nodo perdido todavía.
                    // Si tienen timestamp menor, también se perdieron.
                    // Hay que guardar copias de los sabores antes de perderlos para volver a ponerlos en circulación.
                    // Conectar con el id+2, write_next de este robot y write_previous del robot id+2.
                    // Hacer un while accept incoming en el main para recibir conexiones de robots cuando se cae uno.
                    // Hay que manejar las desconexiones para que no haya panic.
                }
            }

            match message_str.to_string() {
                message_str if message_str.contains("\"OrderRequest\"") => {
                    self.process_order_request(ctx, message_str);
                }
                message_str if message_str.contains("\"OrderPrep\"") => {
                    self.process_order_prep(ctx, message_str);
                }
                message_str if message_str.contains("\"Disconnect\"") => {
                    self.process_disconnect(ctx, message_str);
                }
                message_str if message_str.contains("ACKToken") => {
                    self.process_ack(message_str);
                }
                message_str if message_str.contains("\"Vainilla\"") => {
                    self.process_flavour(ctx, message_str);
                }
                message_str if message_str.contains("\"Dulce de leche\"") => {
                    self.process_flavour(ctx, message_str);
                }
                message_str if message_str.contains("\"Tramontana\"") => {
                    self.process_flavour(ctx, message_str);
                }
                message_str => {
                    println!(
                        "[ROBOT {}] The following unknown message was received:",
                        self.id
                    );
                    println!("[ROBOT {}] {}", self.id, message_str);
                }
            }
        }
    }

    /// A stream connected to this Robot has finished.
    fn finished(&mut self, _ctx: &mut Self::Context) {
        println!("[ROBOT {}] An actor disconnected", self.id);
        // ctx.stop();
    }
}

/// Conects the Robots in the ring.
/// Robots with an even id connect first to the next Robot and then wait for an incoming connection from the previous Robot.
/// Robots with an uneven id wait for an incoming connection from the previous Robot first and then connect to the next Robot.
pub async fn connect_robots(
    id: usize,
) -> (
    (TcpStream, SocketAddr),
    TcpStream,
    HashMap<String, TcpStream>,
) {
    let listener;
    let previous_robot;
    let next_robot;
    if id % 2 == 0 {
        if id == ROBOT_COUNT - 1 {
            next_robot = TcpStream::connect(ROBOT_IP_PREFIX.to_string() + &*(0).to_string())
                .await
                .expect("[ERROR] Couldn't connect to the next Robot");
            println!("[ROBOT {}] Next Robot connected, id {}", id, 0);
        } else {
            next_robot = TcpStream::connect(ROBOT_IP_PREFIX.to_string() + &*(id + 1).to_string())
                .await
                .expect("[ERROR] Couldn't connect to the next Robot");
            println!("[ROBOT {}] Next Robot connected, id {}", id, id + 1);
        }

        listener = TcpListener::bind(ROBOT_IP_PREFIX.to_string() + &*id.to_string())
            .await
            .unwrap();
        previous_robot = listener
            .accept()
            .await
            .expect("[ERROR] Couldn't connect to the previous Robot");
        println!("[ROBOT {}] Previous Robot connected", id);
    } else {
        listener = TcpListener::bind(ROBOT_IP_PREFIX.to_string() + &*id.to_string())
            .await
            .unwrap();
        previous_robot = listener
            .accept()
            .await
            .expect("[ERROR] Couldn't connect to the previous Robot");
        println!("[ROBOT {}] Previous Robot connected", id);

        if id == ROBOT_COUNT - 1 {
            next_robot = TcpStream::connect(ROBOT_IP_PREFIX.to_string() + &*(0).to_string())
                .await
                .expect("[ERROR] Couldn't connect to the next Robot");
            println!("[ROBOT {}] Next Robot connected, id {}", id, 0);
        } else {
            next_robot = TcpStream::connect(ROBOT_IP_PREFIX.to_string() + &*(id + 1).to_string())
                .await
                .expect("[ERROR] Couldn't connect to the next Robot");
            println!("[ROBOT {}] Next Robot connected, id {}", id, id + 1);
        }
    }

    let mut screens = HashMap::new();
    for _ in 0..SCREEN_COUNT {
        let (stream, addr) = listener
            .accept()
            .await
            .expect("[ERROR] Error while connecting to Screen");
        screens.insert(addr.to_string(), stream);
    }

    (previous_robot, next_robot, screens)
}
