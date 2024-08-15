use serde::{Serialize, Deserialize};

use super::order_json::OrderJSON;

// Flags
pub const ORDER_SUCCESS:  u8 = 0;
pub const ORDER_FAILED:   u8 = 1; // No hay helado
pub const ROBOT_OCCUPIED: u8 = 2;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OrderPrep {
    pub message: String, // "OrderPrep"
    pub ip: String,
    pub id: usize,
    pub size: usize,
    pub flavours: Vec<String>,
    pub fail_flag: usize,
}

impl From<OrderJSON> for OrderPrep {
    /// **Obs**: `ip` is left in an invalid state and should be assigned later
    fn from(order_json: OrderJSON) -> Self {
        OrderPrep {
            message: "OrderPrep".to_string(),
            ip: String::new(),
            id: order_json.id,
            size: order_json.size,
            flavours: order_json.flavours,
            fail_flag: 0
        }
    }
}