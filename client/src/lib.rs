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

pub struct ConnHealth<T>(T, u8);

impl<T> ConnHealth<T> {
    pub fn payload(self) -> T {
        self.0
    }

    pub fn retries(&self) -> u8 {
        self.1
    }
}

pub struct Client {
    sock: Option<UdpSocket>,
    retries: u8,
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
    RequestFailed(RequestError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotConnected => write!(f, "client not connected"),
            Error::Io(_) => write!(f, "io error"),
            Error::BadResponse(_) => write!(f, "unexpected response seq no and key"),
            Error::NoResponse(_) => write!(f, "no response from server before timeout"),
            Error::Parse(_) => write!(f, "could not ser/deserialize RPC call"),
            Error::RequestFailed(_) => write!(f, "server saw request but failed to process it"),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Error::NotConnected | Error::BadResponse(_) | Error::NoResponse(_) => None,
            Error::Io(e) => Some(e),
            Error::Parse(p) => Some(p),
            Error::RequestFailed(r) => Some(r),
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

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sock: None,
            retries: 0,
        }
    }

    pub fn connect<S>(
        &mut self,
        addr: S,
        timeout: Option<Duration>,
        retries: u8,
    ) -> Result<(), Error>
    where
        S: ToSocketAddrs,
    {
        let sock = UdpSocket::bind("0.0.0.0:0")?;
        sock.set_read_timeout(timeout)?;
        sock.connect(addr)?;

        self.sock = Some(sock);
        self.retries = retries;

        Ok(())
    }

    pub fn echo<S>(&mut self, msg: S, buf: &mut [u8]) -> Result<ConnHealth<String>, Error>
    where
        S: Into<Echo>,
    {
        let (resp, retries): (EchoResponse, _) =
            self.raw::<Echo, EchoResponse, _, _, _>(ECHO_PATH, msg.into(), buf)?;
        Ok(ConnHealth(resp.0, retries))
    }

    pub fn set_led<LED>(&mut self, set_led: LED, buf: &mut [u8]) -> Result<ConnHealth<()>, Error>
    where
        LED: Into<SetLed>,
    {
        let (resp, retries): (SetLedResponse, _) =
            self.raw::<SetLed, SetLedResponse, _, _, _>(SET_LED_PATH, set_led.into(), buf)?;

        resp.0
            .map(|_| ConnHealth((), retries))
            .map_err(|r| Error::RequestFailed(r))
    }

    pub fn notify<N>(&mut self, notify: N, buf: &mut [u8]) -> Result<ConnHealth<()>, Error>
    where
        N: Into<Notify>,
    {
        let (resp, retries): (NotifyResponse, _) =
            self.raw::<Notify, NotifyResponse, _, _, _>(NOTIFY_PATH, notify.into(), buf)?;

        resp.0
            .map(|_| ConnHealth((), retries))
            .map_err(|r| Error::RequestFailed(r))
    }

    pub fn ack<A>(&mut self, ack: A, buf: &mut [u8]) -> Result<ConnHealth<()>, Error>
    where
        A: Into<Ack>,
    {
        let (resp, retries): (AckResponse, _) =
            self.raw::<Ack, AckResponse, _, _, _>(CLEAR_NOTIFY_PATH, ack.into(), buf)?;

        resp.0
            .map(|_| ConnHealth((), retries))
            .map_err(|r| Error::RequestFailed(r))
    }

    pub fn set_dimming<PWM>(&mut self, pwm: PWM, buf: &mut [u8]) -> Result<ConnHealth<()>, Error>
    where
        PWM: Into<SetDimming>,
    {
        let (resp, retries): (SetDimmingResponse, _) =
            self.raw::<SetDimming, SetDimmingResponse, _, _, _>(SET_DIMMING_PATH, pwm.into(), buf)?;

        resp.0
            .map(|_| ConnHealth((), retries))
            .map_err(|r| Error::RequestFailed(r))
    }

    pub fn set_backlight<B>(&mut self, back: B, buf: &mut [u8]) -> Result<ConnHealth<()>, Error>
    where
        B: Into<SetBacklight>,
    {
        let (resp, retries): (SetBacklightResponse, _) = self
            .raw::<SetBacklight, SetBacklightResponse, _, _, _>(
                HD44780_SET_BACKLIGHT_PATH,
                back.into(),
                buf,
            )?;

        resp.0
            .map(|_| ConnHealth((), retries))
            .map_err(|r| Error::RequestFailed(r))
    }

    pub fn send_msg<M>(&mut self, msg: M, buf: &mut [u8]) -> Result<ConnHealth<()>, Error>
    where
        M: Into<SendMsg>,
    {
        let (resp, retries): (SendMsgResponse, _) =
            self.raw::<SendMsg, SendMsgResponse, _, _, _>(HD44780_SEND_MSG_PATH, msg.into(), buf)?;

        resp.0
            .map(|_| ConnHealth((), retries))
            .map_err(|r| Error::RequestFailed(r))
    }

    pub fn raw<'de, PRQ, PRS, RQ, RS, S>(
        &mut self,
        endpoint: S,
        payload: RQ,
        buf: &'de mut [u8],
    ) -> Result<(RS, u8), Error>
    where
        S: AsRef<str>,
        RQ: Into<PRQ>,
        PRQ: Schema + ser::Serialize,
        PRS: Schema + de::Deserialize<'de> + Into<RS>,
    {
        let key = Key::for_path::<PRQ>(endpoint.as_ref());

        let mut retry = 0;
        let p_payload = payload.into();
        while retry <= self.retries {
            let req = to_slice_keyed(0, key, &p_payload, buf)?;
            self.sock.as_mut().ok_or(Error::NotConnected)?.send(req)?;

            let resp = self.sock.as_mut().ok_or(Error::NotConnected)?.recv(buf);

            if resp.is_ok() {
                break;
            }

            match resp.as_ref().unwrap_err().kind() {
                io::ErrorKind::WouldBlock if retry < self.retries => {
                    retry += 1;
                    continue;
                }
                io::ErrorKind::WouldBlock if retry >= self.retries => {
                    return Err(Error::NoResponse((0, key)))
                }
                _ => return Err(Error::Io(resp.unwrap_err())),
            }
        }

        let (hdr, rest) = extract_header_from_bytes(buf)?;
        if hdr.seq_no == 0 && hdr.key == key {
            let payload = from_bytes::<PRS>(rest)?;
            Ok((payload.into(), retry))
        } else {
            Err(Error::BadResponse((hdr.seq_no, hdr.key)))
        }
    }
}
