pub mod bargraph;
pub mod lcd;

use std::sync::Arc;

use async_lock::Mutex;

pub struct Sensors<'s, I2C> {
    pub bargraph: Option<&'s Arc<Mutex<bargraph::Bargraph<I2C>>>>,
}

impl<'s, I2C> Sensors<'s, I2C> {
    pub fn new() -> Self {
        Self { bargraph: None }
    }
}
