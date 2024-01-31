// use std::any::Any;

use super::bargraph::*;
use wb_notifier_proto::{Device, Driver};

#[derive(Debug)]
pub enum Bargraph {
    Init,
    SetLedNo { num: u8, color: LedColor },
    SetBrightness { pwm: Dimming },
    FastBlink,
    MediumBlink,
    SlowBlink,
    StopBlink,
}

#[derive(Debug)]
pub enum InitFailure {
    Driver(Driver),
    DeviceNotFound(Device),
    RespChannelClosed,
}
