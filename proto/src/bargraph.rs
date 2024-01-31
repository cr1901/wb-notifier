use super::*;

pub const SETLED_PATH: &str = "led/set";

#[derive(Serialize, Deserialize, Schema)]
pub struct SetLed {
    pub row: u8,
    pub col: u8,
}

// This is our Response type
#[derive(Serialize, Deserialize, Schema)]
pub struct SetLedResponse(pub Result<(), ()>);
