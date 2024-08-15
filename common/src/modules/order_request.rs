use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OrderRequest {
    pub message: String, // "OrderRequest"
    pub ip: String,
    pub id: usize,
}

impl OrderRequest {
    pub fn new(ip: String, id: usize) -> Self {
        OrderRequest {
            message: "OrderRequest".to_string(),
            ip,
            id
        }
    }
}