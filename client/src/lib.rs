use std::error;
use std::fmt;
use std::io;
use std::net::{ToSocketAddrs, UdpSocket};
use std::time::Duration;

use postcard::experimental::schema::Schema;
use postcard::{self, from_bytes};
use postcard_rpc::headered::{extract_header_from_bytes, to_slice_keyed};
use postcard_rpc::Key;
use serde::{de, ser};
use wb_notifier_proto::*;

pub struct Client {
    sock: Option<UdpSocket>,
}

#[derive(Debug)]
pub enum Error {
    NotConnected,
    Io(io::Error),
    Parse(postcard::Error),
    BadResponse((u32, Key)),
    NoResponse((u32, Key)),
    // FIXME: Do something like ErrorKind for I/O, getting info from Error
    // socket?
    RequestFailed,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotConnected => write!(f, "client not connected"),
            Error::Io(_) => write!(f, "io error"),
            Error::BadResponse(_) => write!(f, "unexpected response seq no and key"),
            Error::NoResponse(_) => write!(f, "no response from server before timeout"),
            Error::Parse(_) => write!(f, "could not ser/deserialize RPC call"),
            Error::RequestFailed => write!(f, "server could not honor request"),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Error::NotConnected => None,
            Error::Io(e) => Some(e),
            Error::BadResponse(_) => None,
            Error::NoResponse(_) => None,
            Error::Parse(p) => Some(p),
            Error::RequestFailed => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Error::Io(value)
    }
}

impl From<postcard::Error> for Error {
    fn from(value: postcard::Error) -> Self {
        Error::Parse(value)
    }
}

impl Client {
    pub fn new() -> Self {
        Self { sock: None }
    }

    pub fn connect<S>(&mut self, addr: S) -> Result<(), Error>
    where
        S: ToSocketAddrs,
    {
        let sock = UdpSocket::bind("0.0.0.0:0")?;
        sock.set_read_timeout(Some(Duration::from_millis(150)))?;
        sock.connect(addr)?;

        self.sock = Some(sock);

        Ok(())
    }

    pub fn echo<S>(&mut self, msg: S, buf: &mut [u8]) -> Result<String, Error>
    where
        S: Into<Echo>,
    {
        self.raw::<Echo, EchoResponse, _, _, _>(ECHO_PATH, msg.into(), buf)
    }

    pub fn set_led<LED>(&mut self, set_led: LED, buf: &mut [u8]) -> Result<(), Error>
    where
        LED: Into<SetLed>,
    {
        let resp: SetLedResponse =
            self.raw::<SetLed, SetLedResponse, _, _, _>(SET_LED_PATH, set_led.into(), buf)?;

        match resp.0 {
            Ok(()) => Ok(()),
            Err(()) => Err(Error::RequestFailed),
        }
    }

    pub fn set_dimming<PWM>(&mut self, pwm: PWM, buf: &mut [u8]) -> Result<(), Error>
    where
        PWM: Into<SetDimming>,
    {
        let resp: SetDimmingResponse = self.raw::<SetDimming, SetDimmingResponse, _, _, _>(SET_DIMMING_PATH, pwm.into(), buf)?;

        match resp.0 {
            Ok(()) => Ok(()),
            Err(()) => Err(Error::RequestFailed),
        }
    }

    pub fn raw<'de, PRQ, PRS, RQ, RS, S>(
        &mut self,
        endpoint: S,
        payload: RQ,
        buf: &'de mut [u8],
    ) -> Result<RS, Error>
    where
        S: AsRef<str>,
        RQ: Into<PRQ>,
        PRQ: Schema + ser::Serialize,
        PRS: Schema + de::Deserialize<'de> + Into<RS>,
    {
        let key = Key::for_path::<PRQ>(endpoint.as_ref());

        let req = to_slice_keyed(0, key, &payload.into(), buf)?;
        self.sock.as_mut().ok_or(Error::NotConnected)?.send(&req)?;

        self.sock
            .as_mut()
            .ok_or(Error::NotConnected)?
            .recv(buf)
            .map_err(|e| match e.kind() {
                io::ErrorKind::WouldBlock => Error::NoResponse((0, key)),
                _ => Error::Io(e),
            })?;
        let (hdr, rest) = extract_header_from_bytes(buf)?;
        if hdr.seq_no == 0 && hdr.key == key {
            let payload = from_bytes::<PRS>(&rest)?;
            Ok(payload.into())
        } else {
            Err(Error::BadResponse((hdr.seq_no, hdr.key)))
        }
    }
}
