use postcard::experimental::schema::Schema;
use serde::{Serialize, Deserialize};

pub const ECHO: &str = "debug/echo";
pub const HD44780_ENABLE_PATH: &str = "lcd/enable";

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
