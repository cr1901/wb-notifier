use super::*;

pub const SET_LED_PATH: &str = "led/set";
pub const SET_DIMMING_PATH: &str = "led/dimming";

#[derive(Serialize, Deserialize, Schema)]
pub struct SetLed {
    pub num: u8,
    pub color: LedColor
}

// This is our Response type
#[derive(Serialize, Deserialize, Schema)]
pub struct SetLedResponse(pub Result<(), ()>);


#[derive(Serialize, Deserialize, Schema)]
pub enum SetDimming {
    Lo,
    Hi
}

#[derive(Serialize, Deserialize, Schema, Clone, Copy, Debug, PartialEq)]
/// LED colors.
pub enum LedColor {
    /// Turn off both the Red & Green LEDs.
    Off,
    /// Turn on only the Green LED.
    Green,
    /// Turn on only the Red LED.
    Red,
    /// Turn on both the Red  & Green LEDs.
    Yellow,
}

// This is our Response type
#[derive(Serialize, Deserialize, Schema)]
pub struct SetDimmingResponse(pub Result<(), ()>);
