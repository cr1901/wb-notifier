use std::any::Any;

use async_channel::{Sender, Receiver};
use embedded_hal::blocking::i2c::{Write, WriteRead};
use shared_bus::{BusManagerSimple, I2cProxy, NullMutex};

pub mod bargraph;
pub mod cmds;
pub mod lcd;



// pub struct SensorLoop<'a, I2C> {
//     bus: Option<BusManagerSimple<I2C>>,
//     bargraph: Option<bargraph::Bargraph<I2cProxy<'a, NullMutex<I2C>>>>
// }

// impl<'a, I2C, E> SensorLoop<'a, I2C> where I2C: Write<Error = E> + WriteRead<Error = E>, E: 'static {
//     pub fn new(i2c: I2C) -> Self {
//         Self {
//             bus: Some(BusManagerSimple::new(i2c)),
//             bargraph: None
//         }
//     }


struct Sensors<'a, I2C> {
    bargraph: Option<bargraph::Bargraph<I2cProxy<'a, NullMutex<I2C>>>>
}

impl<'a, I2C> Sensors<'a, I2C> {
    fn new() -> Self {
        Self {
            bargraph: None
        }
    }
}

pub fn main_loop<I2C, E>(bus: I2C, cmd: Receiver<(Request, Sender<Response>)>) where I2C: Write<Error = E> + WriteRead<Error = E>, E: 'static {
    let manager = BusManagerSimple::new(bus);
    let mut sensors = Sensors::new();

    loop {
        if let Ok((req, resp)) = cmd.recv_blocking() {
            match req {
                Request::LoopInit => {
                    if let Err(cmds::InitFailure::RespChannelClosed) = init(&manager, &mut sensors, &resp) {
                        break;
                    }
                },
                Request::Bargraph(cmds::Bargraph::SetLed { row, col }) => {
                    let msg: Box<Result<(), _>>;

                    if let Err(_) = sensors.bargraph.as_mut().unwrap().set_led(row, col, true) {
                        msg = Box::new(Err(()));
                    } else {
                        msg = Box::new(Ok(()));
                    }

                    if resp.send_blocking(msg).is_err() {
                        break;
                    }
                }
                _ => unimplemented!()
            }
        }
    }
}

fn init<'a, I2C, E>(manager: &'a BusManagerSimple<I2C>, sensors: &mut Sensors<'a, I2C>, resp: &Sender<Response>) -> Result<(), cmds::InitFailure> where I2C: Write<Error = E> + WriteRead<Error = E>, E: 'static {
    let i2c = manager.acquire_i2c();

    let mut bg = bargraph::Bargraph::new(i2c, 0x70);
    if let Err(e) = bg.initialize() {
        match e {
            bargraph::Error::Hal(_) =>{
                let msg: Box<Result<(), _>> = Box::new(Err(cmds::InitFailure::Bargraph));
                if resp.send_blocking(msg).is_err() {
                    return Err(cmds::InitFailure::RespChannelClosed);
                } else {
                    return Err(cmds::InitFailure::Bargraph);
                }
            },
            bargraph::Error::OutOfRange => unreachable!()
        }
        
    }

    if let Err(e) = bg.set_dimming(bargraph::Dimming::BRIGHTNESS_3_16) {
        match e {
            bargraph::Error::Hal(_) =>{
                
                let msg: Box<Result<(), _>> = Box::new(Err(cmds::InitFailure::Bargraph));
                if resp.send_blocking(msg).is_err() {
                    return Err(cmds::InitFailure::RespChannelClosed);
                } else {
                    return Err(cmds::InitFailure::Bargraph);
                }
            },
            bargraph::Error::OutOfRange => unreachable!()
        }
        
    }

    sensors.bargraph = Some(bg);
    let msg: Box<Result<(), cmds::InitFailure>> = Box::new(Ok(()));
    if resp.send_blocking(msg).is_err() {
        return Err(cmds::InitFailure::RespChannelClosed);
    } else {
        return Ok(())
    }
}

pub enum Request {
    LoopInit,
    Bargraph(cmds::Bargraph),
    LoopDeinit
}

pub type Response = Box<dyn Any + Send>;
