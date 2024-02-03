// use std::any::Any;

use std::{error, fmt};

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
    // FIXME: Unused placeholder. Errors need to be reworked.
    DeviceNotFound(Device),
    RespChannelClosed,
}

impl fmt::Display for InitFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InitFailure::Driver(d) => {
                let drv = match d {
                    Driver::Bargraph => "bargraph",
                    Driver::Hd44780 => "lcd"
                };

                write!(f, "driver {drv} could not communicate with device")
            }
            InitFailure::DeviceNotFound(Device { name: _name, addr: _addr, driver: _driver }) => write!(f, "unused placeholder"),
            InitFailure::RespChannelClosed => write!(f, "sensor thread failed to initialize"),
        }
    }
}

impl error::Error for InitFailure {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        None
    }
}
