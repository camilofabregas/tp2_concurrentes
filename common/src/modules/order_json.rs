use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub struct OrderJSON {
    pub id: usize,
    pub size: usize,
    pub flavours: Vec<String>
}
