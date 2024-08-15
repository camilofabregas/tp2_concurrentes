use std::{fs::OpenOptions, io::{Error, Write}};
use actix::Message;
use serde::{Serialize, Deserialize};
use chrono::Local;

use super::order_prep::OrderPrep;

#[derive(Serialize, Deserialize, Debug)]
#[derive(Message)]
#[rtype(result = "usize")]
/// PaymentConfirmation struct has information of the sender (ip & id), the message (PaymentCapture) and the copy itself (to log to disk).
pub struct PaymentConfirmation {
    pub ip: String,
    pub id: String,
    pub message: String,
    pub order_data: OrderPrep,
}

impl PaymentConfirmation {
    /// Create a new PaymentConfirmation instance.
    pub fn new(_ip: String, _id: String, order_data: OrderPrep) -> Self {
        PaymentConfirmation { 
            ip: _ip, 
            id: _id, 
            message: "PaymentConfirmation".to_string(),
            order_data 
        }
    }
    
    /// Confirm a payment, by logging the order and screen information to disk.
    pub fn confirm_payment(confirmation: &Self) -> Result<(), Error> {
        let confirmation_str = confirmation.id.clone() + &" completed order of size ".to_string() +  &confirmation.order_data.size.to_string() + &" with flavours ".to_string() + &confirmation.order_data.flavours.join(",").to_string();
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open("log.txt")?;
        let log_str = Local::now().format("%Y-%m-%d %H:%M:%S").to_string() + " " + &confirmation_str;
        writeln!(file, "{}", log_str)?;

        Ok(())
    }
}