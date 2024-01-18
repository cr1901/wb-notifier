use async_net::UdpSocket;
use std::error;
use std::fmt;
use std::io;

pub struct Server {

}

#[derive(Debug)]
pub enum Error {
    Io(io::Error)
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(_) => write!(f, "io error")
        }
    }
}


impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Io(v) => Some(v)
        }
    }    
}

impl Server {
    pub fn new() -> Self {
        Self {

        }
    }

    pub async fn main_loop(self) -> Result<(), Error> {
        let socket = UdpSocket::bind("0.0.0.0:12000").await?;
        let mut buf = vec![0u8; 1024];

        loop {
            let (n, addr) = socket.recv_from(&mut buf).await?;
            socket.send_to(&buf[..n], &addr).await?;
        }
    }
}
