use async_channel::{bounded, Sender};
use async_executor::LocalExecutor;
use async_net::{SocketAddr, UdpSocket};
use linux_embedded_hal::I2cdev;
use postcard_rpc::{self, endpoint, Dispatch, Key, WireHeader};
use serde::Deserialize;

use std::any::Any;
use std::error;
use std::fmt;
use std::future::Future;
use std::io;
use std::rc::Rc;
use std::thread;

use wb_notifier_driver::cmds::{self, InitFailure};
use wb_notifier_driver::bargraph;
use wb_notifier_driver::{self, Request, Response};
use wb_notifier_proto::*;

endpoint!(EchoEndpoint, Echo, EchoResponse, "debug/echo");
endpoint!(SetLedEndpoint, SetLed, SetLedResponse, "led/set");
endpoint!(SetDimmingEndpoint, SetDimming, SetDimmingResponse, "led/dimming");

pub struct Server {
    addr: SocketAddr,
    devices: Vec<Device>,
}

type AsyncSend = Sender<(Request, Sender<Response>)>;

struct Context<'ex, 'b> {
    ex: &'b Rc<LocalExecutor<'ex>>,
    sock: UdpSocket,
    addr: Option<SocketAddr>,
    send: Option<AsyncSend>,
}

impl<'ex, 'b> Context<'ex, 'b> {
    fn new(ex: &'b Rc<LocalExecutor<'ex>>, sock: UdpSocket) -> Self {
        Self {
            ex,
            sock,
            addr: None,
            send: None,
        }
    }
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Init(&'static str),
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
            Self::NoMatch { key, seq_no } => write!(
                f,
                "cannot dispatch sequence no {} with key {:?}",
                seq_no, key
            ),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Io(v) => Some(v),
            Self::Init(s) => {
                let box_err = Box::<dyn error::Error + 'static>::from(*s);
                Some(Box::<dyn error::Error + 'static>::leak(box_err))
            }
            Self::Parse(p) => Some(p),
            Self::NoMatch { key: _, seq_no: _ } => None,
        }
    }
}

impl Server {
    pub fn new(addr: SocketAddr, devices: Vec<Device>) -> Self {
        Self { addr, devices }
    }

    fn send_init_msg(&self, send: &AsyncSend, dev: &Device) -> Result<Box<dyn Any + Send>, Error> {
        let (resp_send, resp_recv) = bounded(1);
        send.send_blocking((Request::Init(dev.clone()), resp_send))
            .map_err(|_| Error::Init("sensor thread closed send channel"))?;

        let init_res = resp_recv
            .recv_blocking()
            .map_err(|_| Error::Init("did not receive response from sensor thread"))?;

        Ok(init_res)
    }

    pub async fn main_loop(self, ex: Rc<LocalExecutor<'_>>) -> Result<(), Error> {
        let socket = UdpSocket::bind(self.addr).await?;
        let mut buf = vec![0u8; 1024];
        let mut dispatch = Dispatch::<Context, Error, 16>::new(Context::new(&ex, socket.clone()));

        let i2c = I2cdev::new("/dev/i2c-1").map_err(|e| Error::Io(e.into()))?;
        let (sensor_send, sensor_recv) = bounded(16);

        thread::spawn(move || wb_notifier_driver::main_loop(i2c, sensor_recv));

        self.devices
            .iter()
            .map(|d| {
                let init_res = self.send_init_msg(&sensor_send, d)?;

                if let Err(e) = init_res
                    .downcast::<Result<(), InitFailure>>()
                    .as_deref()
                    .unwrap()
                {
                    println!("{:?}", e);
                    return Err(Error::Init("sensor thread failed to initialize"));
                }

                Ok(())
            })
            .collect::<Result<Vec<()>, _>>()?;

        dispatch
            .add_handler::<EchoEndpoint>(echo_handler)
            .map_err(|s| Error::Init(s))?;
        dispatch
            .add_handler::<SetLedEndpoint>(set_led_handler)
            .map_err(|s| Error::Init(s))?;
        dispatch
            .add_handler::<SetDimmingEndpoint>(set_dimming_handler)
            .map_err(|s| Error::Init(s))?;
        dispatch.context().send = Some(sensor_send);

        loop {
            let (n, addr) = socket.recv_from(&mut buf).await?;
            dispatch.context().addr = Some(addr);
            match dispatch.dispatch(&mut buf[..n]) {
                Ok(_) => {}
                Err(e) => {
                    println!("Need to handle error: {:?}", e)
                }
            }
        }
    }
}

fn deserialize_detach<'ex, 'de, T, F, H>(
    ex: Rc<LocalExecutor<'ex>>,
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

fn set_led_handler<'ex, 'b>(
    hdr: &WireHeader,
    ctx: &mut Context<'ex, 'b>,
    bytes: &[u8],
) -> Result<(), Error> {
    deserialize_detach(ctx.ex.clone(), bytes, |msg| {
        set_led_task(
            ctx.ex.clone(),
            hdr.seq_no,
            hdr.key,
            (ctx.sock.clone(), ctx.addr.unwrap().clone()),
            ctx.send.clone().unwrap(),
            msg,
        )
    })
}

async fn set_led_task<'a, 'ex>(
    _ex: Rc<LocalExecutor<'ex>>,
    seq_no: u32,
    key: Key,
    (sock, addr): (UdpSocket, SocketAddr),
    req_send: AsyncSend,
    SetLed { num, color }: SetLed,
) {
    let mut buf = vec![0u8; 1024];

    let (resp_send, resp_recv) = bounded(1);

    // For now, we give up on any send/recv/downcast/deserialize errors and
    // rely on client to time out.
    let _ = req_send
        .send((
            Request::Bargraph(cmds::Bargraph::SetLedNo { num, color }),
            resp_send,
        ))
        .await;

    let res = resp_recv
        .recv()
        .await
        .map(|r| r.downcast::<Result<(), ()>>().unwrap());

    if let Ok(resp) = res.as_deref() {
        if let Ok(used) = postcard_rpc::headered::to_slice_keyed(seq_no, key, resp, &mut buf) {
            let _ = sock.send_to(used, addr).await;
        }
    }
}

fn set_dimming_handler<'ex, 'b>(
    hdr: &WireHeader,
    ctx: &mut Context<'ex, 'b>,
    bytes: &[u8],
) -> Result<(), Error> {
    deserialize_detach(ctx.ex.clone(), bytes, |msg| {
        set_dimming_task(
            ctx.ex.clone(),
            hdr.seq_no,
            hdr.key,
            (ctx.sock.clone(), ctx.addr.unwrap().clone()),
            ctx.send.clone().unwrap(),
            msg,
        )
    })
}

async fn set_dimming_task<'a, 'ex>(
    _ex: Rc<LocalExecutor<'ex>>,
    seq_no: u32,
    key: Key,
    (sock, addr): (UdpSocket, SocketAddr),
    req_send: AsyncSend,
    dimming: SetDimming
) {
    let mut buf = vec![0u8; 1024];

    let (resp_send, resp_recv) = bounded(1);

    let req = match dimming {
        SetDimming::Hi => {
            bargraph::Dimming::BRIGHTNESS_16_16
        },
        SetDimming::Lo => {
            bargraph::Dimming::BRIGHTNESS_16_16
        },
    };

    // For now, we give up on any send/recv/downcast/deserialize errors and
    // rely on client to time out.
    let _ = req_send
        .send((
            Request::Bargraph(cmds::Bargraph::SetBrightness { pwm: req }),
            resp_send,
        ))
        .await;

    let res = resp_recv
        .recv()
        .await
        .map(|r| r.downcast::<Result<(), ()>>().unwrap());

    if let Ok(resp) = res.as_deref() {
        if let Ok(used) = postcard_rpc::headered::to_slice_keyed(seq_no, key, resp, &mut buf) {
            let _ = sock.send_to(used, addr).await;
        }
    }
}

fn echo_handler<'ex, 'b>(
    hdr: &WireHeader,
    ctx: &mut Context<'ex, 'b>,
    bytes: &[u8],
) -> Result<(), Error> {
    deserialize_detach(ctx.ex.clone(), bytes, |msg| {
        echo_task(
            ctx.ex.clone(),
            hdr.seq_no,
            hdr.key,
            (ctx.sock.clone(), ctx.addr.unwrap().clone()),
            msg,
        )
    })
}

async fn echo_task<'a, 'ex>(
    _ex: Rc<LocalExecutor<'ex>>,
    seq_no: u32,
    key: Key,
    (sock, addr): (UdpSocket, SocketAddr),
    msg: String,
) {
    let resp = EchoResponse(msg.to_uppercase());
    let mut buf = vec![0u8; 1024];

    if let Ok(used) = postcard_rpc::headered::to_slice_keyed(seq_no, key, &resp, &mut buf) {
        let _ = sock.send_to(used, addr).await;
    }
}
