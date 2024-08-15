use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Disconnect {
    pub ip: String,
    pub id: String,
    pub message: String
}

impl Disconnect {
    pub fn new(_ip: String, _id: String) -> Self {
        Disconnect { ip: _ip, id: _id, message: "Disconnect".to_string() }
    }
}