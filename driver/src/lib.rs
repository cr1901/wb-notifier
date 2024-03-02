use std::{any::Any, error};

use async_channel::{Receiver, Sender};
use embedded_hal::blocking::i2c::{Write, WriteRead};
use shared_bus::{BusManagerSimple, I2cProxy, NullMutex};
use wb_notifier_proto::{Device, Driver};

pub mod bargraph;
pub mod cmds;
pub mod lcd;

use cmds::{Error, InitFailure, RequestError};

pub type AsyncRecv = Receiver<(Request, Sender<Response>)>;
pub type Response = Result<Box<dyn Any + Send>, Error>;

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

enum InternalError {
    /// Uninitialized Device, etc. The loop should keep going.
    Transient,
    /// Lost channel connection, etc. The loop should end.
    Persistent,
}

pub fn main_loop<I2C, I2CE>(bus: I2C, cmd: &AsyncRecv)
where
    I2C: Write<Error = I2CE> + WriteRead<Error = I2CE>,
    I2CE: 'static + Send + error::Error,
{
    let manager = BusManagerSimple::new(bus);
    let mut sensors = Sensors::new();

    loop {
        if let Err(InternalError::Persistent) = main_loop_single_iter(cmd, &manager, &mut sensors) {
            break;
        }
    }
}

fn main_loop_single_iter<'a, 'b, I2C, I2CE>(
    cmd: &AsyncRecv,
    manager: &'b BusManagerSimple<I2C>,
    sensors: &mut Sensors<'a, I2C>,
) -> Result<(), InternalError>
where
    I2C: Write<Error = I2CE> + WriteRead<Error = I2CE>,
    I2CE: 'static + Send + error::Error,
    'b: 'a,
{
    if let Ok((req, resp)) = cmd.recv_blocking() {
        match req {
            #[allow(clippy::single_match)]
            Request::Init(d) => match d {
                Device {
                    name: _,
                    addr,
                    driver: Driver::Bargraph,
                } => {
                    if let Err(InitFailure::RespChannelClosed) =
                        bargraph_init(manager, sensors, &resp, addr)
                    {
                        return Err(InternalError::Persistent);
                    }
                }
                _ => {}
            },
            Request::Bargraph(cmds::Bargraph::SetBrightness { pwm }) => {
                do_bargraph(&resp, sensors, |bg| bg.set_dimming(pwm))?;
            }
            Request::Bargraph(cmds::Bargraph::SetLedNo { num, color }) => {
                do_bargraph(&resp, sensors, |bg| bg.set_led_no(num, color))?;
            }
            Request::Bargraph(cmds::Bargraph::FastBlink) => {
                do_bargraph(&resp, sensors, |bg| {
                    bg.set_display(bargraph::Display::TWO_HZ)
                })?;
            }
            Request::Bargraph(cmds::Bargraph::MediumBlink) => {
                do_bargraph(&resp, sensors, |bg| {
                    bg.set_display(bargraph::Display::ONE_HZ)
                })?;
            }
            Request::Bargraph(cmds::Bargraph::SlowBlink) => {
                do_bargraph(&resp, sensors, |bg| {
                    bg.set_display(bargraph::Display::HALF_HZ)
                })?;
            }
            Request::Bargraph(cmds::Bargraph::StopBlink) => {
                do_bargraph(&resp, sensors, |bg| bg.set_display(bargraph::Display::ON))?;
            }
            Request::Bargraph(cmds::Bargraph::ClearAll) => {
                do_bargraph(&resp, sensors, |bg| bg.clear_all())?;
            }
            _ => unimplemented!(),
        }

        Ok(())
    } else {
        Err(InternalError::Persistent)
    }
}

fn bargraph_init<'a, I2C, I2CE>(
    manager: &'a BusManagerSimple<I2C>,
    sensors: &mut Sensors<'a, I2C>,
    resp: &Sender<Response>,
    addr: u8,
) -> Result<(), InitFailure>
where
    I2C: Write<Error = I2CE> + WriteRead<Error = I2CE>,
    I2CE: 'static + Send + error::Error,
{
    let i2c = manager.acquire_i2c();

    let mut bg = bargraph::Bargraph::new(i2c, addr);
    if let Err(e) = bg.initialize() {
        match e {
            bargraph::Error::Hal(_) => {
                let _err_channel_err: Box<dyn error::Error + Send> =
                    Box::new(InitFailure::Driver(Driver::Bargraph));

                let client_err = Err(cmds::Error::Init(InitFailure::Driver(Driver::Bargraph)));

                if resp.send_blocking(client_err).is_err() {
                    return Err(InitFailure::RespChannelClosed);
                }

                return Err(InitFailure::Driver(Driver::Bargraph));
            }
            bargraph::Error::OutOfRange => unreachable!(),
        }
    }

    if let Err(e) = bg.set_dimming(bargraph::Dimming::BRIGHTNESS_3_16) {
        match e {
            bargraph::Error::Hal(_) => {
                let _err_channel_err: Box<dyn error::Error + Send> =
                    Box::new(InitFailure::Driver(Driver::Bargraph));

                let client_err = Err(cmds::Error::Init(InitFailure::Driver(Driver::Bargraph)));

                if resp.send_blocking(client_err).is_err() {
                    return Err(InitFailure::RespChannelClosed);
                }

                return Err(InitFailure::Driver(Driver::Bargraph));
            }
            bargraph::Error::OutOfRange => unreachable!(),
        }
    }

    sensors.bargraph = Some(bg);
    let ok_msg: Result<Box<dyn Any + Send>, _> = Ok(Box::new(()));
    if resp.send_blocking(ok_msg).is_err() {
        Err(InitFailure::RespChannelClosed)
    } else {
        Ok(())
    }
}

fn do_bargraph<'a, 'bg, I2C, I2CE, T, E, F>(
    resp: &Sender<Result<Box<dyn Any + Send>, Error>>,
    sensors: &'bg mut Sensors<'a, I2C>,
    mut req: F,
) -> Result<(), InternalError>
where
    I2C: Write<Error = I2CE> + WriteRead<Error = I2CE>,
    I2CE: 'static,
    F: FnMut(&'bg mut bargraph::Bargraph<I2cProxy<'a, NullMutex<I2C>>>) -> Result<T, E>,
    T: Send + 'static,
    E: error::Error + Send + 'static,
{
    let bg = sensors.bargraph.as_mut().ok_or(InternalError::Transient)?;

    let client_msg: Result<Box<dyn Any + Send>, _> = match req(bg) {
        Ok(t) => Ok(Box::new(t)),
        Err(e) => {
            let _err_channel_err: Box<dyn error::Error + Send> = Box::new(e);
            Err(Error::Client(RequestError {}))
        }
    };

    if resp.send_blocking(client_msg).is_err() {
        return Err(InternalError::Persistent);
    }

    Ok(())
}
