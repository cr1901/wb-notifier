use postcard::experimental::schema::Schema;
use serde::{Serialize, Deserialize};

pub const ECHO: &str = "debug/echo";
pub const HD44780_ENABLE_PATH: &str = "lcd/enable";
pub const SETLED_PATH: &str = "led/set";

#[derive(Serialize, Deserialize, Schema)]
pub struct Echo(pub String);

// This is our Response type
#[derive(Serialize, Deserialize, Schema)]
pub struct EchoResponse(pub String);


// This is our Request type
#[derive(Serialize, Deserialize, Schema)]
pub struct Enable();

// This is our Response type
#[derive(Serialize, Deserialize, Schema)]
pub struct EnableResponse(Result<(), ()>);


#[derive(Serialize, Deserialize, Schema)]
pub struct SetLed {
    pub row: u8,
    pub col: u8
}

// This is our Response type
#[derive(Serialize, Deserialize, Schema)]
pub struct SetLedResponse(pub Result<(), ()>);
