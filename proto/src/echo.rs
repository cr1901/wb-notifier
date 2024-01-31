use super::*;

pub const ECHO_PATH: &str = "debug/echo";

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
