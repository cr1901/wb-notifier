// use std::any::Any;

use super::bargraph::*;
use super::Response;
use ht16k33::Dimming;
use wb_notifier_proto::{SetLed, SetLedResponse};

// pub struct CmdResponse<T, E>(async_channel::Sender<Result<T, E>>);

// impl<T, E> CmdResponse<T, E> {
//     pub fn new(send: async_channel::Sender<Result<T, E>>) -> Self {
//         Self(send)
//     }
// }

#[derive(Debug)]
pub enum Bargraph {
    Init,
    SetLed { row: u8, col: u8 },
    ClearLed { row: u8, col: u8 },
    SetLedNo { num: u8, color: LedColor },
    SetBrightness { pwm: Dimming },
    StartBlink,
    MediumBlink,
    SlowBlink,
    StopBlink,
}

#[derive(Debug)]
pub enum InitFailure {
    Bargraph,
    RespChannelClosed,
}

// impl From<(SetLed, CmdResponse<Result<(),()>>)> for BargraphCmd {
//     fn from((SetLed { row, col}, resp): (SetLed, CmdResponse<Result<(),()>>)) -> Self {
//         BargraphCmd::SetLed { row, col, resp }
//     }
// }
