pub use super::*;

#[derive(Debug, Serialize, Deserialize, Schema, Hash, Clone)]
pub struct Device {
    // TODO: In principle, we could have dynamic endpoints and distinguish multiple
    // of the same devices by this name String. But right now, it does nothing.
    pub name: String,
    pub addr: u8,
    pub driver: Driver,
}

// TODO: Parameterize based on an InitFailure type?
#[derive(Serialize, Deserialize, Schema)]
pub struct InitResponse<E>(pub Result<(), E>);

#[derive(Debug, Serialize, Deserialize, Schema, Hash, Clone)]
pub enum Driver {
    Bargraph,
    Hd44780,
}
