pub mod bargraph;
pub mod lcd;

use std::sync::Arc;

use async_lock::Mutex;
use embedded_hal::blocking::i2c::{Write, WriteRead};

pub struct Sensors<'s, I2C, D> where I2C: Write + WriteRead {
    pub bargraph: Option<&'s Arc<Mutex<bargraph::Bargraph<I2C>>>>,
    pub lcd: Option<&'s Arc<Mutex<lcd::Lcd<I2C, D>>>>,
}

impl<'s, I2C, D> Sensors<'s, I2C, D> where I2C: Write + WriteRead {
    pub fn new() -> Self {
        Self { 
            bargraph: None,
            lcd: None 
        }
    }
}
