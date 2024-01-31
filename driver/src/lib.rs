use std::any::Any;

use async_channel::{Receiver, Sender};
use embedded_hal::blocking::i2c::{Write, WriteRead};
use shared_bus::{BusManagerSimple, I2cProxy, NullMutex};
use wb_notifier_proto::{Device, Driver};

pub mod bargraph;
pub mod cmds;
pub mod lcd;

pub type AsyncRecv = Receiver<(Request, Sender<Response>)>;
pub type Response = Box<dyn Any + Send>;

struct Sensors<'a, I2C> {
    bargraph: Option<bargraph::Bargraph<I2cProxy<'a, NullMutex<I2C>>>>,
}

impl<'a, I2C> Sensors<'a, I2C> {
    fn new() -> Self {
        Self { bargraph: None }
    }
}

pub enum Request {
    Init(Device),
    Bargraph(cmds::Bargraph),
    LoopDeinit,
}

pub fn main_loop<I2C, E>(bus: I2C, cmd: AsyncRecv)
where
    I2C: Write<Error = E> + WriteRead<Error = E>,
    E: 'static,
{
    let manager = BusManagerSimple::new(bus);
    let mut sensors = Sensors::new();

    loop {
        if let Ok((req, resp)) = cmd.recv_blocking() {
            match req {
                Request::Init(d) => {
                    match d {
                        Device { name: _, addr, driver: Driver::Bargraph } => {
                            if let Err(cmds::InitFailure::RespChannelClosed) =
                                bargraph_init(&manager, &mut sensors, &resp, addr)
                            {
                                break;
                            }
                        }
                        _ => {}
                    }
                },
                Request::Bargraph(cmds::Bargraph::SetLed { row, col }) => {
                    let msg: Box<Result<(), _>>;

                    let bg = match sensors.bargraph.as_mut() {
                        Some(bg) => bg,
                        None => {
                            // TODO: Create an error type for UninitializedDevice
                            // or similar.
                            continue
                        }
                    };

                    if let Err(_) = bg.set_led(row, col, true) {
                        msg = Box::new(Err(()));
                    } else {
                        msg = Box::new(Ok(()));
                    }

                    if resp.send_blocking(msg).is_err() {
                        break;
                    }
                }
                _ => unimplemented!(),
            }
        }
    }
}

fn bargraph_init<'a, I2C, E>(
    manager: &'a BusManagerSimple<I2C>,
    sensors: &mut Sensors<'a, I2C>,
    resp: &Sender<Response>,
    addr: u8,
) -> Result<(), cmds::InitFailure>
where
    I2C: Write<Error = E> + WriteRead<Error = E>,
    E: 'static,
{
    let i2c = manager.acquire_i2c();

    let mut bg = bargraph::Bargraph::new(i2c, addr);
    if let Err(e) = bg.initialize() {
        match e {
            bargraph::Error::Hal(_) => {
                let msg: Box<Result<(), _>> = Box::new(Err(cmds::InitFailure::Driver(Driver::Bargraph)));
                if resp.send_blocking(msg).is_err() {
                    return Err(cmds::InitFailure::RespChannelClosed);
                } else {
                    return Err(cmds::InitFailure::Driver(Driver::Bargraph));
                }
            }
            bargraph::Error::OutOfRange => unreachable!(),
        }
    }

    if let Err(e) = bg.set_dimming(bargraph::Dimming::BRIGHTNESS_3_16) {
        match e {
            bargraph::Error::Hal(_) => {
                let msg: Box<Result<(), _>> = Box::new(Err(cmds::InitFailure::Driver(Driver::Bargraph)));
                if resp.send_blocking(msg).is_err() {
                    return Err(cmds::InitFailure::RespChannelClosed);
                } else {
                    return Err(cmds::InitFailure::Driver(Driver::Bargraph));
                }
            }
            bargraph::Error::OutOfRange => unreachable!(),
        }
    }

    sensors.bargraph = Some(bg);
    let msg: Box<Result<(), cmds::InitFailure>> = Box::new(Ok(()));
    if resp.send_blocking(msg).is_err() {
        return Err(cmds::InitFailure::RespChannelClosed);
    } else {
        return Ok(());
    }
}

