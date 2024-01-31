use super::*;

pub const SET_LED_PATH: &str = "led/set";
pub const SET_DIMMING_PATH: &str = "led/dimming";

#[derive(Serialize, Deserialize, Schema)]
pub struct SetLed {
    pub row: u8,
    pub col: u8,
}

// This is our Response type
#[derive(Serialize, Deserialize, Schema)]
pub struct SetLedResponse(pub Result<(), ()>);


#[derive(Serialize, Deserialize, Schema)]
pub enum SetDimming {
    Lo,
    Hi
}

// This is our Response type
#[derive(Serialize, Deserialize, Schema)]
pub struct SetDimmingResponse(pub Result<(), ()>);
