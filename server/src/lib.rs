use async_channel::{bounded, Sender};
use async_executor::LocalExecutor;
use async_lock::Mutex;
use async_net::{SocketAddr, UdpSocket};

use embedded_hal::blocking::delay::{DelayMs, DelayUs};
use embedded_hal::blocking::i2c::{Write, WriteRead};
use linux_embedded_hal::{Delay, I2cdev};
use postcard_rpc::{self, endpoint, Dispatch, Key, WireHeader};
use serde::Deserialize;
use wb_notifier_driver::bargraph::{Bargraph, Dimming};
use wb_notifier_driver::lcd::Lcd;
use wb_notifier_driver::Sensors;

use std::error;
use std::fmt;
use std::future::Future;
use std::io;
use std::rc::Rc;
use std::sync::Arc;

use wb_notifier_driver;
use wb_notifier_proto::*;

mod tasks;

endpoint!(EchoEndpoint, Echo, EchoResponse, "debug/echo");
endpoint!(SetLedEndpoint, SetLed, SetLedResponse, "led/set");
endpoint!(
    SetDimmingEndpoint,
    SetDimming,
    SetDimmingResponse,
    "led/dimming"
);
endpoint!(NotifyEndpoint, Notify, NotifyResponse, "led/notify");
endpoint!(AckEndpoint, Ack, AckResponse, "led/ack");
endpoint!(
    SetBacklightEndpoint,
    SetBacklight,
    SetBacklightResponse,
    "lcd/backlight"
);
endpoint!(SendMsgEndpoint, SendMsg, SendMsgResponse, "lcd/msg");

pub struct Server {
    addr: SocketAddr,
    devices: Vec<Device>,
}

struct Context<'ex, 'b, I2C, D>
where
    I2C: Write + WriteRead,
{
    ex: &'b Rc<LocalExecutor<'ex>>,
    sock: UdpSocket,
    addr: Option<SocketAddr>,
    blink_send: Option<Sender<tasks::background::BlinkInfo>>,
    sensors: Sensors<'b, I2C, D>,
}

impl<'ex, 'b, I2C, D> Context<'ex, 'b, I2C, D>
where
    I2C: Write + WriteRead,
{
    fn new(ex: &'b Rc<LocalExecutor<'ex>>, sock: UdpSocket) -> Self {
        Self {
            ex,
            sock,
            addr: None,
            blink_send: None,
            sensors: Sensors::new(),
        }
    }
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Init(InitError),
    Parse(postcard::Error),
    NoMatch { key: Key, seq_no: u32 },
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(_) => write!(f, "io error"),
            Self::Init(_) => write!(f, "initialization error"),
            Self::Parse(_) => write!(f, "error deserializing postcard message"),
            Self::NoMatch { key, seq_no } => {
                write!(f, "cannot dispatch sequence no {seq_no} with key {key:?}")
            }
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Io(v) => Some(v),
            Self::Init(i) => Some(i),
            Self::Parse(p) => Some(p),
            Self::NoMatch { key: _, seq_no: _ } => None,
        }
    }
}

#[derive(Debug)]
pub enum InitError {
    Driver(Driver),
    Dispatch(&'static str),
}

impl fmt::Display for InitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InitError::Driver(d) => {
                let drv = match d {
                    Driver::Bargraph => "bargraph",
                    Driver::Hd44780 => "lcd",
                };

                write!(f, "driver {drv} could not communicate with device")
            }
            InitError::Dispatch(_) => write!(f, "dispatch table failed to initialize"),
        }
    }
}

impl error::Error for InitError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            InitError::Driver(_) => None,
            InitError::Dispatch(d) => {
                let box_err = Box::<dyn error::Error + 'static>::from(*d);
                Some(Box::<dyn error::Error + 'static>::leak(box_err))
            }
        }
    }
}

impl Server {
    #[must_use]
    pub fn new(addr: SocketAddr, devices: Vec<Device>) -> Self {
        Self { addr, devices }
    }

    pub async fn main_loop(self, ex: Rc<LocalExecutor<'_>>) -> Result<(), Error> {
        let socket = UdpSocket::bind(self.addr).await?;
        let mut buf = vec![0u8; 1024];

        let mut bg = None;
        let mut lcd = None;
        let mut dispatch =
            Dispatch::<Context<_, _>, Error, 16>::new(Context::new(&ex, socket.clone()));

        let i2c = I2cdev::new("/dev/i2c-1").map_err(|e| Error::Io(e.into()))?;
        let bus: &'static _ = shared_bus::new_std!(I2cdev = i2c).unwrap();

        self.devices
            .iter()
            .map(|d| {
                // Self::send_init_msg(&sensor_send, d)?;

                match d.driver {
                    Driver::Bargraph => {
                        let arc_bg = Arc::new(Mutex::new(Bargraph::new(bus.acquire_i2c(), d.addr)));
                        {
                            let mut bg = arc_bg.try_lock_arc().unwrap();

                            bg.initialize()
                                .map_err(|_| Error::Init(InitError::Driver(Driver::Bargraph)))?;

                            bg.set_dimming(Dimming::BRIGHTNESS_3_16)
                                .map_err(|_| Error::Init(InitError::Driver(Driver::Bargraph)))?;
                        }

                        let (blink_send, blink_recv) = bounded(1);
                        ex.spawn(tasks::background::blink(
                            ex.clone(),
                            arc_bg.clone(),
                            blink_recv,
                        ))
                        .detach();

                        bg.replace(arc_bg);
                        dispatch.context().blink_send = Some(blink_send);
                    }
                    Driver::Hd44780 => {
                        let arc_lcd;
                        {
                            let delay = Delay {};
                            let lcd = Lcd::new(bus.acquire_i2c(), delay, d.addr)
                                .map_err(|_| Error::Init(InitError::Driver(Driver::Hd44780)))?;

                            arc_lcd = Arc::new(Mutex::new(lcd));
                            {
                                let mut lcd = arc_lcd.try_lock_arc().unwrap();

                                lcd.initialize()
                                    .map_err(|_| Error::Init(InitError::Driver(Driver::Hd44780)))?;
                            }
                        }

                        lcd.replace(arc_lcd);
                    }
                }

                Ok(())
            })
            .collect::<Result<Vec<()>, Error>>()?;

        dispatch.context().sensors.bargraph = bg.as_ref();
        dispatch.context().sensors.lcd = lcd.as_ref();

        dispatch
            .add_handler::<EchoEndpoint>(echo_handler)
            .map_err(|e| Error::Init(InitError::Dispatch(e)))?;
        dispatch
            .add_handler::<SetLedEndpoint>(set_led_handler)
            .map_err(|e| Error::Init(InitError::Dispatch(e)))?;
        dispatch
            .add_handler::<SetDimmingEndpoint>(set_dimming_handler)
            .map_err(|e| Error::Init(InitError::Dispatch(e)))?;
        dispatch
            .add_handler::<NotifyEndpoint>(notify_handler)
            .map_err(|e| Error::Init(InitError::Dispatch(e)))?;
        dispatch
            .add_handler::<AckEndpoint>(ack_handler)
            .map_err(|e| Error::Init(InitError::Dispatch(e)))?;
        dispatch
            .add_handler::<SetBacklightEndpoint>(set_backlight_handler)
            .map_err(|e| Error::Init(InitError::Dispatch(e)))?;
        dispatch
            .add_handler::<SendMsgEndpoint>(send_msg_handler)
            .map_err(|e| Error::Init(InitError::Dispatch(e)))?;

        loop {
            let (n, addr) = socket.recv_from(&mut buf).await?;
            dispatch.context().addr = Some(addr);
            match dispatch.dispatch(&buf[..n]) {
                Ok(()) => {}
                Err(e) => {
                    println!("Need to handle error: {e:?}");
                }
            }
        }

        #[allow(unreachable_code)]
        Ok(())
    }
}

fn deserialize_detach<'ex, 'de, T, F, H>(
    ex: &Rc<LocalExecutor<'ex>>,
    bytes: &'de [u8],
    task: T,
) -> Result<(), Error>
where
    T: FnOnce(H) -> F,
    F: Future<Output = ()> + 'ex,
    H: Deserialize<'de>,
{
    match postcard::from_bytes::<H>(bytes) {
        Ok(msg) => {
            ex.spawn(task(msg)).detach();
            Ok(())
        }
        Err(e) => Err(Error::Parse(e)),
    }
}

fn set_led_handler<I2C, E, D>(
    hdr: &WireHeader,
    ctx: &mut Context<'_, '_, I2C, D>,
    bytes: &[u8],
) -> Result<(), Error>
where
    I2C: Send + Write<Error = E> + WriteRead<Error = E> + 'static,
    E: Send + 'static,
{
    deserialize_detach(ctx.ex, bytes, |msg| {
        tasks::handlers::set_led(
            ctx.ex.clone(),
            hdr.seq_no,
            hdr.key,
            (ctx.sock.clone(), ctx.addr.unwrap()),
            ctx.sensors.bargraph.unwrap().clone(),
            msg,
        )
    })
}

fn set_dimming_handler<I2C, E, D>(
    hdr: &WireHeader,
    ctx: &mut Context<'_, '_, I2C, D>,
    bytes: &[u8],
) -> Result<(), Error>
where
    I2C: Send + Write<Error = E> + WriteRead<Error = E> + 'static,
    E: Send + 'static,
{
    deserialize_detach(ctx.ex, bytes, |msg| {
        tasks::handlers::set_dimming(
            ctx.ex.clone(),
            hdr.seq_no,
            hdr.key,
            (ctx.sock.clone(), ctx.addr.unwrap()),
            ctx.sensors.bargraph.unwrap().clone(),
            msg,
        )
    })
}

fn notify_handler<I2C, E, D>(
    hdr: &WireHeader,
    ctx: &mut Context<'_, '_, I2C, D>,
    bytes: &[u8],
) -> Result<(), Error>
where
    I2C: Send + Write<Error = E> + WriteRead<Error = E> + 'static,
    E: Send + 'static,
{
    deserialize_detach(ctx.ex, bytes, |msg| {
        tasks::handlers::notify(
            ctx.ex.clone(),
            hdr.seq_no,
            hdr.key,
            (ctx.sock.clone(), ctx.addr.unwrap()),
            ctx.blink_send.clone().unwrap(),
            ctx.sensors.bargraph.unwrap().clone(),
            msg,
        )
    })
}

fn ack_handler<I2C, E, D>(
    hdr: &WireHeader,
    ctx: &mut Context<'_, '_, I2C, D>,
    bytes: &[u8],
) -> Result<(), Error>
where
    I2C: Send + Write<Error = E> + WriteRead<Error = E> + 'static,
    E: Send + 'static,
{
    deserialize_detach(ctx.ex, bytes, |msg| {
        tasks::handlers::ack(
            ctx.ex.clone(),
            hdr.seq_no,
            hdr.key,
            (ctx.sock.clone(), ctx.addr.unwrap()),
            ctx.blink_send.clone().unwrap(),
            ctx.sensors.bargraph.unwrap().clone(),
            msg,
        )
    })
}

fn echo_handler<I2C, D, E>(
    hdr: &WireHeader,
    ctx: &mut Context<'_, '_, I2C, D>,
    bytes: &[u8],
) -> Result<(), Error>
where
    I2C: Send + Write<Error = E> + WriteRead<Error = E> + 'static,
    E: Send + 'static,
{
    deserialize_detach(ctx.ex, bytes, |msg| {
        tasks::handlers::echo(
            ctx.ex.clone(),
            hdr.seq_no,
            hdr.key,
            (ctx.sock.clone(), ctx.addr.unwrap()),
            msg,
        )
    })
}

fn set_backlight_handler<I2C, E, D>(
    hdr: &WireHeader,
    ctx: &mut Context<'_, '_, I2C, D>,
    bytes: &[u8],
) -> Result<(), Error>
where
    I2C: Send + Write<Error = E> + WriteRead<Error = E> + 'static,
    E: Send + 'static,
    D: DelayMs<u8> + DelayUs<u16> + Send + 'static,
{
    deserialize_detach(ctx.ex, bytes, |msg| {
        tasks::handlers::set_backlight(
            ctx.ex.clone(),
            hdr.seq_no,
            hdr.key,
            (ctx.sock.clone(), ctx.addr.unwrap()),
            ctx.sensors.lcd.unwrap().clone(),
            msg,
        )
    })
}

fn send_msg_handler<I2C, E, D>(
    hdr: &WireHeader,
    ctx: &mut Context<'_, '_, I2C, D>,
    bytes: &[u8],
) -> Result<(), Error>
where
    I2C: Send + Write<Error = E> + WriteRead<Error = E> + 'static,
    E: Send + 'static,
    D: DelayMs<u8> + DelayUs<u16> + Send + 'static,
{
    deserialize_detach(ctx.ex, bytes, |msg| {
        tasks::handlers::send_msg(
            ctx.ex.clone(),
            hdr.seq_no,
            hdr.key,
            (ctx.sock.clone(), ctx.addr.unwrap()),
            ctx.sensors.lcd.unwrap().clone(),
            msg,
        )
    })
}
