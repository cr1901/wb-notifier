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

enum Error {
    /// Uninitialized Device, etc. The loop should keep going.
    Transient,
    /// Lost channel connection, etc. The loop should end.
    Persistent,
}

pub fn main_loop<I2C, E>(bus: I2C, cmd: AsyncRecv)
where
    I2C: Write<Error = E> + WriteRead<Error = E>,
    E: 'static + Send,
{
    let manager = BusManagerSimple::new(bus);
    let mut sensors = Sensors::new();

    loop {
        if let Err(Error::Persistent) = main_loop_single_iter(&cmd, &manager, &mut sensors) {
            break;
        }
    }
}

fn main_loop_single_iter<'a, 'b, I2C, E>(
    cmd: &AsyncRecv,
    manager: &'b BusManagerSimple<I2C>,
    sensors: &mut Sensors<'a, I2C>,
) -> Result<(), Error>
where
    I2C: Write<Error = E> + WriteRead<Error = E>,
    E: 'static + Send,
    'b: 'a,
{
    if let Ok((req, resp)) = cmd.recv_blocking() {
        match req {
            Request::Init(d) => match d {
                Device {
                    name: _,
                    addr,
                    driver: Driver::Bargraph,
                } => {
                    if let Err(cmds::InitFailure::RespChannelClosed) =
                        bargraph_init(&manager, sensors, &resp, addr)
                    {
                        return Err(Error::Persistent);
                    }
                }
                _ => {}
            },
            Request::Bargraph(cmds::Bargraph::SetBrightness { pwm }) => {
                do_bargraph(resp, sensors, |bg| bg.set_dimming(pwm))?;
            }
            Request::Bargraph(cmds::Bargraph::SetLedNo { num, color }) => {
                do_bargraph(resp, sensors, |bg| bg.set_led_no(num, color))?;
            }
            Request::Bargraph(cmds::Bargraph::FastBlink) => {
                do_bargraph(resp, sensors, |bg| {
                    bg.set_display(bargraph::Display::TWO_HZ)
                })?;
            }
            Request::Bargraph(cmds::Bargraph::MediumBlink) => {
                do_bargraph(resp, sensors, |bg| {
                    bg.set_display(bargraph::Display::ONE_HZ)
                })?;
            }
            Request::Bargraph(cmds::Bargraph::SlowBlink) => {
                do_bargraph(resp, sensors, |bg| {
                    bg.set_display(bargraph::Display::HALF_HZ)
                })?;
            }
            Request::Bargraph(cmds::Bargraph::StopBlink) => {
                do_bargraph(resp, sensors, |bg| bg.set_display(bargraph::Display::ON))?;
            }
            _ => unimplemented!(),
        }

        Ok(())
    } else {
        Err(Error::Persistent)
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
    E: 'static + Send,
{
    let i2c = manager.acquire_i2c();

    let mut bg = bargraph::Bargraph::new(i2c, addr);
    if let Err(e) = bg.initialize() {
        match e {
            bargraph::Error::Hal(_) => {
                let msg: Box<Result<(), _>> =
                    Box::new(Err(cmds::InitFailure::Driver(Driver::Bargraph)));
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
                let msg: Box<Result<(), _>> =
                    Box::new(Err(cmds::InitFailure::Driver(Driver::Bargraph)));
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

fn do_bargraph<'a, 'bg, I2C, I2CE, T, E, F>(
    resp: Sender<Box<dyn Any + Send>>,
    sensors: &'bg mut Sensors<'a, I2C>,
    mut req: F,
) -> Result<(), Error>
where
    I2C: Write<Error = I2CE> + WriteRead<Error = I2CE>,
    I2CE: 'static,
    F: FnMut(&'bg mut bargraph::Bargraph<I2cProxy<'a, NullMutex<I2C>>>) -> Result<T, E>,
    T: Send + 'static,
    E: Send + 'static,
{
    let bg = sensors.bargraph.as_mut().ok_or(Error::Transient)?;

    let msg = Box::new(req(bg));

    if resp.send_blocking(msg).is_err() {
        return Err(Error::Persistent);
    }

    Ok(())
}
