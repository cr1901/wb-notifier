use eyre::Result;

#[cfg(feature = "client")]
mod client {
    use std::fmt::Write;
    use std::net::{AddrParseError, SocketAddr};

    pub use wb_notifier_client::Client;
    pub use wb_notifier_proto::*;

    pub use argh::{self, FromArgs};

    #[derive(FromArgs)]
    /// Workbench notifier client
    pub struct ClientArgs {
        /// address to connect to
        #[argh(positional, from_str_fn(sock_parse))]
        pub addr: SocketAddr,
        #[argh(subcommand)]
        pub cmd: Cmd
    }

    #[derive(FromArgs, PartialEq, Debug)]
    #[argh(subcommand)]
    pub enum Cmd {
        Notify(NotifySubCommand),
        Ack(AckSubCommand),
        ConfigBargraph(ConfigBargraphSubCommand)
    }

    #[derive(FromArgs, PartialEq, Debug)]
    #[argh(subcommand, name="notify")]
    /// notify workbench daemon with new message
    pub struct NotifySubCommand {
        /// message number/LED to bind to
        #[argh(option, short='l')]
        pub num: Option<u8>,
        /// status level of message
        #[argh(option, short='s', from_str_fn(status_parse))]
        pub status: Option<Status>,
        /// message to send to LCD
        #[argh(option, short='m')]
        pub msg: Option<String>
    }

    #[derive(FromArgs, PartialEq, Debug)]
    #[argh(subcommand, name="ack")]
    /// clear message from workbench daemon
    pub struct AckSubCommand {
        #[argh(option, short='l')]
        /// message number/LED to bind to clear
        pub num: Option<u8>
    }

    #[derive(FromArgs, PartialEq, Debug)]
    #[argh(subcommand, name="config-bg")]
    /// bargraph config
    pub struct ConfigBargraphSubCommand {
        #[argh(option, short='d', from_str_fn(dim_parse))]
        /// message number/LED to bind to clear
        pub level: Option<SetDimming>
    }

    fn sock_parse(addr: &str) -> Result<SocketAddr, String> {
        addr.parse().map_err(|e: AddrParseError| e.to_string())
    }

    fn dim_parse(level: &str) -> Result<SetDimming, String> {
        match level {
            "hi" | "high" => Ok(SetDimming::Hi),
            "lo" | "low" => Ok(SetDimming::Lo),
            _ => {
                let mut msg = String::new();
                let _ = write!(msg, r#"expected "hi", "high", "lo", or "low", got {}"#, level);
                Err(msg)
            }
        }
    }

    fn status_parse(status: &str) -> Result<Status, String> {
        match status {
            "red" | "urgent" => return Ok(Status::Error),
            "yellow" | "error" => return Ok(Status::Warning),
            "on" | "green" | "ok" => return Ok(Status::Ok),
            _ => {}
        }

        status.parse().map(|u: u16| {
            if u == 0 {
                Status::Ok
            } else {
                Status::Warning
            }
        }).map_err(|e| {
            let mut msg = String::new();
            let _ = write!(msg, r#"expected "red", "urgent", "yellow", "error", "on", "green", "ok", or integer, got {}"#, status);
            msg
        })
    }
}

#[cfg(feature = "client")]
use client::*;

#[cfg(feature = "client")]
fn main() -> Result<()> {
    let args: ClientArgs = argh::from_env();

    let mut client = Client::new();
    client.connect(args.addr)?;

    let mut buf = vec![0; 1024];

    match args.cmd {
        Cmd::Notify(NotifySubCommand {
            num,
            status,
            msg
        }) => {
            client.notify(
                Notify {
                    num: num.unwrap_or(0),
                    status: status.unwrap_or(Status::Ok)
                },
                &mut buf,
            )?;
        },
        Cmd::Ack(AckSubCommand {
            num,
        }) => {
            client.ack(
                Ack {
                    num: num.unwrap_or(0)
                },
                &mut buf,
            )?;
        },
        Cmd::ConfigBargraph(ConfigBargraphSubCommand {
            level
        }) => {
            client.set_dimming(level.unwrap_or(SetDimming::Hi), &mut buf)?;
        },
    }

    Ok(())
}

#[cfg(not(feature = "client"))]
fn main() -> Result<()> {
    println!("client feature not enabled");

    Ok(())
}
