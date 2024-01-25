use async_channel::{bounded, Sender, Receiver};
use async_net::{UdpSocket, SocketAddr};
use async_executor::LocalExecutor;
use linux_embedded_hal::I2cdev;
use postcard_rpc::{Endpoint, Key};
use postcard_rpc::{self, endpoint};
use postcard_rpc::Dispatch;
use postcard_rpc::WireHeader;
use wb_notifier_driver::cmds::InitFailure;

use std::any::Any;
use std::error;
use std::fmt;
use std::io;
use std::rc::Rc;
use std::thread;

use wb_notifier_proto::*;
use wb_notifier_driver::{self, Request};

endpoint!(EchoEndpoint, Echo, EchoResponse, "debug/echo");
endpoint!(SetLedEndpoint, SetLed, SetLedResponse, "led/set");

pub struct Server {
    addr: Option<SocketAddr>
}

struct Context<'ex, 'b> {
    ex: &'b Rc<LocalExecutor<'ex>>,
    sock: UdpSocket,
    addr: Option<SocketAddr>,
    send: Option<Sender<(Request, Sender::<Box<dyn Any + Send>>)>>,
    resp: Option<(Sender<Box<dyn Any + Send>>, Receiver<Box<dyn Any + Send>>)>
}

impl<'ex, 'b> Context<'ex, 'b> {
    fn new(ex: &'b Rc<LocalExecutor<'ex>>, sock: UdpSocket) -> Self {
        Self {
            ex,
            sock,
            addr: None,
            send: None,
            resp: None
        }
    }
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Init(&'static str),
    Parse(postcard::Error),
    NoMatch {
        key: Key,
        seq_no: u32
    },
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
            Self::NoMatch { key, seq_no } => write!(f, "cannot dispatch sequence no {} with key {:?}", seq_no, key),
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
            },
            Self::Parse(p) => Some(p),
            Self::NoMatch{key: _, seq_no: _} => None,
        }
    }    
}

impl Server {
    pub fn new() -> Self {
        Self {
            addr: None
        }
    }

    pub fn set_addr<S>(&mut self, addr: S) where S: Into<SocketAddr> {
        self.addr = Some(addr.into());
    }

    pub async fn main_loop(self, ex: Rc<LocalExecutor<'_>>) -> Result<(), Error> {
        let default_addr: SocketAddr = "0.0.0.0:12000".parse().unwrap();
        let socket = UdpSocket::bind(self.addr.unwrap_or(default_addr)).await?;
        let mut buf = vec![0u8; 1024];
        let mut dispatch = Dispatch::<Context, Error, 16>::new(Context::new(&ex, socket.clone()));

        let i2c = I2cdev::new("/dev/i2c-1").map_err(|e| Error::Io(e.into()))?;
        let (sensor_send, sensor_recv) = bounded(16);

        thread::spawn(move || wb_notifier_driver::main_loop(i2c, sensor_recv));

        let (resp_send, resp_recv) = bounded(1);
        sensor_send.send((Request::LoopInit, resp_send)).await.map_err(|_| Error::Init("sensor thread closed send channel"))?;
        let init_res = resp_recv.recv().await.unwrap().downcast::<Result<(), InitFailure>>().unwrap();

        if let Err(e) = *init_res {
            println!("{:?}", e);
            return Err(Error::Init("sensor thread failed to initialize"));
        }

        dispatch.add_handler::<EchoEndpoint>(echo_handler).map_err(|s| Error::Init(s))?;
        dispatch.add_handler::<SetLedEndpoint>(set_led_handler).map_err(|s| Error::Init(s))?;
        dispatch.context().send = Some(sensor_send);

        loop {
            let (n, addr) = socket.recv_from(&mut buf).await?;
            dispatch.context().addr = Some(addr);
            dispatch.context().resp = Some(bounded(1));
            match dispatch.dispatch(&mut buf[..n]) {
                Ok(_) => {},
                Err(e) => { todo!("Need to handle error: {:?}", e) },
            }
        }
    }
}

fn set_led_handler<'ex, 'b>(hdr: &WireHeader, ctx: &mut Context<'ex, 'b>, bytes: &[u8]) -> Result<(), Error> {
    let Context { ex, sock, addr , send, resp } = ctx;
    let addr = addr.unwrap().clone();
    let sock = sock.clone();
    let send = send.clone().unwrap();
    // let ctx = ctx.clone();

    match postcard::from_bytes::<SetLed>(bytes) {
        Ok(msg) => {
            ex.spawn(set_led_task(ex.clone(), hdr.seq_no, hdr.key, (sock, addr), send, msg)).detach();
            Ok(())
        },
        Err(e) => {
            Err(Error::Parse(e))
        }
    }
}


async fn set_led_task<'a, 'ex>(_ex: Rc<LocalExecutor<'ex>>, seq_no: u32, key: Key, (sock, addr): (UdpSocket, SocketAddr),
 req_send: Sender<(Request, Sender::<Box<dyn Any + Send>>)>, SetLed { row, col}: SetLed ) {
    let mut buf = vec![0u8; 1024];

    let (resp_send, resp_recv) = bounded(1);
    
    req_send.send((Request::Bargraph(wb_notifier_driver::cmds::Bargraph::SetLed { row, col }), resp_send)).await.unwrap();

    let resp = resp_recv.recv().await.unwrap().downcast::<Result<(), ()>>().unwrap();

    if let Ok(used) = postcard_rpc::headered::to_slice_keyed(seq_no, key, &*resp, &mut buf) {
        let _ = sock.send_to(used, addr).await;
    }
}


fn echo_handler<'ex, 'b>(hdr: &WireHeader, ctx: &mut Context<'ex, 'b>, bytes: &[u8]) -> Result<(), Error> {
    let Context { ex, sock, addr , .. } = ctx;
    let addr = addr.unwrap().clone();
    let sock = sock.clone();
    // let ctx = ctx.clone();

    match postcard::from_bytes::<Echo>(bytes) {
        Ok(msg) => {
            ex.spawn(echo_task(ex.clone(), hdr.seq_no, hdr.key, (sock, addr), msg.0)).detach();
            Ok(())
        },
        Err(e) => {
            Err(Error::Parse(e))
        }
    }
}

async fn echo_task<'a, 'ex>(_ex: Rc<LocalExecutor<'ex>>, seq_no: u32, key: Key, (sock, addr): (UdpSocket, SocketAddr), msg: String) {
    let resp = EchoResponse(msg.to_uppercase());
    let mut buf = vec![0u8; 1024];

    if let Ok(used) = postcard_rpc::headered::to_slice_keyed(seq_no, key, &resp, &mut buf) {
        let _ = sock.send_to(used, addr).await;
    }
}
