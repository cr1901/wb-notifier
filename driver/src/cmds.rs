// use std::any::Any;

use super::bargraph::*;
use ht16k33::Dimming;

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
