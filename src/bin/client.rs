use eyre::Result;

#[cfg(feature = "client")]
mod client {
    use std::fmt::Write;
    pub use std::net::{AddrParseError, SocketAddr};
    pub use std::time::Duration;
    pub use eyre::Context;
    pub use std::env;

    use fundu::DurationParser;
    pub use wb_notifier_client::Client;
    pub use wb_notifier_proto::*;

    pub use argh::{self, FromArgs};

    #[derive(FromArgs)]
    /// Workbench notifier client
    pub struct ClientArgs {
        /// address to connect to
        #[argh(positional, from_str_fn(sock_parse))]
        pub addr: Option<SocketAddr>,
        /// timeout for receive socket
        #[argh(option, short = 't', from_str_fn(duration_parse))]
        pub timeout: Option<Duration>,
        #[argh(subcommand)]
        pub cmd: Cmd,
    }

    #[derive(FromArgs, PartialEq, Debug)]
    #[argh(subcommand)]
    pub enum Cmd {
        Notify(NotifySubCommand),
        Ack(AckSubCommand),
        ConfigBargraph(ConfigBargraphSubCommand),
        ConfigLcd(ConfigLcdSubCommand),
    }

    #[derive(FromArgs, PartialEq, Debug)]
    #[argh(subcommand, name = "notify")]
    /// notify workbench daemon with new message
    pub struct NotifySubCommand {
        /// message number/LED to bind to
        #[argh(option, short = 'l')]
        pub num: Option<u8>,
        /// status level of message
        #[argh(option, short = 's', from_str_fn(status_parse))]
        pub status: Option<Status>,
        /// message to send to LCD
        #[argh(option, short = 'm')]
        pub msg: Option<String>,
    }

    #[derive(FromArgs, PartialEq, Debug)]
    #[argh(subcommand, name = "ack")]
    /// clear message from workbench daemon
    pub struct AckSubCommand {
        #[argh(option, short = 'l')]
        /// message number/LED to bind to clear; clears all without this option
        pub num: Option<u8>,
    }

    #[derive(FromArgs, PartialEq, Debug)]
    #[argh(subcommand, name = "config-bg")]
    /// bargraph config
    pub struct ConfigBargraphSubCommand {
        #[argh(option, short = 'd', from_str_fn(dim_parse))]
        /// message number/LED to bind to clear
        pub level: Option<SetDimming>,
    }

    #[derive(FromArgs, PartialEq, Debug)]
    #[argh(subcommand, name = "config-lcd")]
    /// lcd config
    pub struct ConfigLcdSubCommand {
        #[argh(option, short = 'b', from_str_fn(backlight_parse))]
        /// message number/LED to bind to clear
        pub back: Option<SetBacklight>,
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
                let _ = write!(
                    msg,
                    r#"expected "hi", "high", "lo", or "low", got {}"#,
                    level
                );
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
        }).map_err(|_e| {
            let mut msg = String::new();
            let _ = write!(msg, r#"expected "red", "urgent", "yellow", "error", "on", "green", "ok", or integer, got {}"#, status);
            msg
        })
    }

    fn duration_parse(timeout: &str) -> Result<Duration, String> {
        let parser = DurationParser::new();
        Duration::try_from(parser.parse(timeout).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    }

    fn backlight_parse(level: &str) -> Result<SetBacklight, String> {
        match level {
            "off" => Ok(SetBacklight::Off),
            "on" => Ok(SetBacklight::On),
            _ => {
                let mut msg = String::new();
                let _ = write!(msg, r#"expected "on", or "off", got {}"#, level);
                Err(msg)
            }
        }
    }
}

#[cfg(feature = "client")]
use client::*;

#[cfg(feature = "client")]
fn main() -> Result<()> {
    let args: ClientArgs = argh::from_env();

    let addr = match args.addr {
        Some(a) => a,
        None => env::var("WBN_SERVER_ADDR")
            .wrap_err("if an addr is not provided, environment variable WBN_SERVER_ADDR must be set")?
            .parse()?,
    };

    let mut client = Client::new();
    client.connect(addr, args.timeout.or(Some(Duration::from_millis(1000))))?;

    let mut buf = vec![0; 1024];

    match args.cmd {
        Cmd::Notify(NotifySubCommand { num, status, msg }) => {
            client.notify(
                Notify {
                    num: num.unwrap_or(0),
                    status: status.unwrap_or(Status::Ok),
                },
                &mut buf,
            )?;

            if let Some(m) = msg {
                client.send_msg(SendMsg(m), &mut buf)?;
            }
        }
        Cmd::Ack(AckSubCommand { num }) => {
            client.ack(Ack { num }, &mut buf)?;
        }
        Cmd::ConfigBargraph(ConfigBargraphSubCommand { level }) => {
            client.set_dimming(level.unwrap_or(SetDimming::Hi), &mut buf)?;
        }
        Cmd::ConfigLcd(ConfigLcdSubCommand { back }) => {
            client.set_backlight(back.unwrap_or(SetBacklight::On), &mut buf)?;
        }
    }

    Ok(())
}

#[cfg(not(feature = "client"))]
fn main() -> Result<()> {
    println!("client feature not enabled");

    Ok(())
}
