use postcard::experimental::schema::Schema;
use serde::{Deserialize, Serialize};

pub const ECHO: &str = "debug/echo";
pub const HD44780_ENABLE_PATH: &str = "lcd/enable";
pub const SETLED_PATH: &str = "led/set";

#[derive(Serialize, Deserialize, Schema)]
pub struct Echo(pub String);

impl From<&str> for Echo {
    fn from(value: &str) -> Self {
        Echo(value.to_string())
    }
}

// This is our Response type
#[derive(Serialize, Deserialize, Schema)]
pub struct EchoResponse(pub String);

impl From<EchoResponse> for String {
    fn from(value: EchoResponse) -> Self {
        value.0
    }
}

// This is our Request type
#[derive(Serialize, Deserialize, Schema)]
pub struct Enable();

// This is our Response type
#[derive(Serialize, Deserialize, Schema)]
pub struct EnableResponse(Result<(), ()>);

#[derive(Serialize, Deserialize, Schema)]
pub struct SetLed {
    pub row: u8,
    pub col: u8,
}

// This is our Response type
#[derive(Serialize, Deserialize, Schema)]
pub struct SetLedResponse(pub Result<(), ()>);

/* TODO: "Get Error" endpoint... something like
#[derive(Serialize, Deserialize, Schema)]
pub struct ErrorQuery {
    pub seq_no: u32,
    pub key: Key,
}

#[derive(Serialize, Deserialize, Schema)]
pub struct LastErrorResponse(pub Option<DispatchError>);

#[derive(Serialize, Deserialize, Schema)]
pub enum DispatchError {
    NonexistentEndpoint
}
*/
