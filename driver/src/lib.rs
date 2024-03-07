pub mod bargraph;
pub mod lcd;

use std::sync::Arc;

use async_lock::Mutex;
use embedded_hal::blocking::i2c::{Write, WriteRead};

pub struct Sensors<'s, I2C, D>
where
    I2C: Write + WriteRead,
{
    pub bargraph: Option<&'s Arc<Mutex<bargraph::Bargraph<I2C>>>>,
    pub lcd: Option<&'s Arc<Mutex<lcd::Lcd<I2C, D>>>>,
}

impl<'s, I2C, D> Default for Sensors<'s, I2C, D>
where
    I2C: Write + WriteRead,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<'s, I2C, D> Sensors<'s, I2C, D>
where
    I2C: Write + WriteRead,
{
    #[must_use]
    pub fn new() -> Self {
        Self {
            bargraph: None,
            lcd: None,
        }
    }
}
