use serde::{Deserialize, Serialize};

/// Flavour structure.
/// It is passed down to the next Robot in the ring to be consumed if needed.
#[derive(Serialize, Deserialize, Debug)]
pub struct Flavour {
    pub name: String,
    pub amount: f64,
}

impl Flavour {
    /// Flavour constructor.
    pub fn new(name: String, amount: f64) -> Self {
        Flavour { name, amount }
    }
}
