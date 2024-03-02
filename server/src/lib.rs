use async_channel::{bounded, Sender};
use async_channel::{RecvError, SendError};
use async_executor::LocalExecutor;
use async_lock::Mutex;
use async_net::{SocketAddr, UdpSocket};

use embedded_hal::blocking::i2c::{Write, WriteRead};
use linux_embedded_hal::I2cdev;
use postcard_rpc::{self, endpoint, Dispatch, Key, WireHeader};
use serde::Deserialize;
use wb_notifier_driver::bargraph::Bargraph;

use std::error;
use std::fmt;
use std::future::Future;
use std::io;
use std::rc::Rc;
use std::sync::Arc;

use wb_notifier_driver::cmds::InitFailure;
use wb_notifier_driver::{self, Request, Response};
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

pub struct Server {
    addr: SocketAddr,
    devices: Vec<Device>,
}

type AsyncSend = Sender<(Request, Sender<Response>)>;

struct Context<'ex, 'b, I2C> {
    ex: &'b Rc<LocalExecutor<'ex>>,
    sock: UdpSocket,
    addr: Option<SocketAddr>,
    send: Option<AsyncSend>,
    blink_send: Option<Sender<tasks::background::BlinkInfo>>,
    bg: Option<&'b Arc<Mutex<Bargraph<I2C>>>>,
}

impl<'ex, 'b, I2C> Context<'ex, 'b, I2C> {
    fn new(ex: &'b Rc<LocalExecutor<'ex>>, sock: UdpSocket) -> Self {
        Self {
            ex,
            sock,
            addr: None,
            send: None,
            blink_send: None,
            bg: None,
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
    SendChannel(SendError<(Request, Sender<Response>)>),
    RecvChannel(RecvError),
    DriverThread(InitFailure),
    Dispatch(&'static str),
}

impl fmt::Display for InitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InitError::SendChannel(_) => write!(f, "sensor thread closed send channel"),
            InitError::RecvChannel(_) => write!(f, "did not receive response from sensor thread"),
            InitError::DriverThread(_) => write!(f, "sensor thread failed to initialize"),
            InitError::Dispatch(_) => write!(f, "dispatch table failed to initialize"),
        }
    }
}

impl error::Error for InitError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            InitError::SendChannel(s) => Some(s),
            InitError::RecvChannel(r) => Some(r),
            InitError::DriverThread(d) => Some(d),
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
        let mut dispatch =
            Dispatch::<Context<_>, Error, 16>::new(Context::new(&ex, socket.clone()));

        let i2c = I2cdev::new("/dev/i2c-1").map_err(|e| Error::Io(e.into()))?;
        let (sensor_send, sensor_recv) = bounded(16);
        let bus: &'static _ = shared_bus::new_std!(I2cdev = i2c).unwrap();

        self.devices
            .iter()
            .map(|d| {
                // Self::send_init_msg(&sensor_send, d)?;

                match d.driver {
                    Driver::Bargraph => {
                        let arc_bg = Arc::new(Mutex::new(Bargraph::new(bus.acquire_i2c(), d.addr)));
                        arc_bg.try_lock_arc().unwrap().initialize().map_err(|_| {
                            Error::Init(InitError::DriverThread(InitFailure::Driver(
                                Driver::Bargraph,
                            )))
                        })?;

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
                        unimplemented!()
                    }
                }

                Ok(())
            })
            .collect::<Result<Vec<()>, Error>>()?;

        dispatch.context().bg = bg.as_ref();

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
        dispatch.context().send = Some(sensor_send);

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

fn set_led_handler<I2C, E>(
    hdr: &WireHeader,
    ctx: &mut Context<'_, '_, I2C>,
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
            ctx.bg.unwrap().clone(),
            msg,
        )
    })
}

fn set_dimming_handler<I2C, E>(
    hdr: &WireHeader,
    ctx: &mut Context<'_, '_, I2C>,
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
            ctx.bg.unwrap().clone(),
            msg,
        )
    })
}

fn notify_handler<I2C, E>(
    hdr: &WireHeader,
    ctx: &mut Context<'_, '_, I2C>,
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
            ctx.bg.unwrap().clone(),
            msg,
        )
    })
}

fn ack_handler<I2C, E>(
    hdr: &WireHeader,
    ctx: &mut Context<'_, '_, I2C>,
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
            ctx.bg.unwrap().clone(),
            msg,
        )
    })
}

fn echo_handler<I2C>(
    hdr: &WireHeader,
    ctx: &mut Context<'_, '_, I2C>,
    bytes: &[u8],
) -> Result<(), Error> {
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
