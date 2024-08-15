use actix::Message;
use serde::{Serialize, Deserialize};
use rand::Rng;

const CAPTURE_PROBABILITY: usize = 90;

#[derive(Serialize, Deserialize, Debug)]
#[derive(Message)]
#[rtype(result = "usize")]
/// PaymentCapture struct has information of the sender (ip & id), the message (PaymentCapture) and a valid flag that is sent on 'true' by default.
pub struct PaymentCapture {
    pub ip: String,
    pub id: String,
    pub message: String,
    pub valid: bool
}

impl PaymentCapture {
    /// Create a new PaymentCapture instance.
    pub fn new(_ip: String, _id: String, _valid: bool) -> Self {
        PaymentCapture { ip: _ip, id: _id, message: "PaymentCapture".to_string(), valid: _valid }
    }
    
    /// Capture a payment, with a 10% probability of failing.
    pub fn capture_payment(mut capture: Self, new_id: &str, new_ip: &str) -> Self {
        let mut rng = rand::thread_rng();
        if rng.gen_range(0..100) >= CAPTURE_PROBABILITY {
            capture.valid = false;
        }
        capture.id = new_id.to_string();
        capture.ip = new_ip.to_string();
        capture
    }
}