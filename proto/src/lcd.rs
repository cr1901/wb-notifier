use super::*;

pub const HD44780_ENABLE_PATH: &str = "lcd/enable";

// This is our Request type
#[derive(Serialize, Deserialize, Schema)]
pub struct Enable();

// This is our Response type
#[derive(Serialize, Deserialize, Schema)]
pub struct EnableResponse(Result<(), RequestError>);
