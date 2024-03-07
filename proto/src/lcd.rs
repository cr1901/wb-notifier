use super::*;

pub const HD44780_SET_BACKLIGHT_PATH: &str = "lcd/backlight";
pub const HD44780_SEND_MSG_PATH: &str = "lcd/msg";

// This is our Request type
#[derive(Serialize, Deserialize, Schema)]
pub struct Enable();

// This is our Response type
#[derive(Serialize, Deserialize, Schema)]
pub struct EnableResponse(Result<(), RequestError>);

#[derive(Serialize, Deserialize, Schema, PartialEq, Debug)]
pub enum SetBacklight {
    On,
    Off,
}

#[derive(Serialize, Deserialize, Schema)]
pub struct SetBacklightResponse(pub Result<(), RequestError>);

impl From<Result<(), RequestError>> for SetBacklightResponse {
    fn from(value: Result<(), RequestError>) -> Self {
        Self(value)
    }
}

#[derive(Serialize, Deserialize, Schema)]
pub struct SendMsg(pub String);

#[derive(Serialize, Deserialize, Schema)]
pub struct SendMsgResponse(pub Result<MsgStatus, RequestError>);

#[derive(Serialize, Deserialize, Schema)]
pub enum MsgStatus {
    Ok,
    Truncated,
}
