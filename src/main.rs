use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::thread;

use argh::FromArgs;
use config::Config;
use directories::ProjectDirs;
use eyre::{bail, eyre, Result};
use jsonrpsee::server::{RpcModule, ServerBuilder, ServerHandle};
use linux_embedded_hal::I2cdev;
use serde::Deserialize;
use tokio::{runtime, sync};
use wb_notifier::bargraph::{Bargraph, LedColor};

#[derive(Deserialize, Hash)]
struct WbInfo {
    devices: Vec<Device>
}

#[derive(Deserialize, Hash)]
struct Device {
    name: String,
    addr: u8,
    driver: Driver,
}

#[derive(Deserialize, Hash)]
enum Driver {
    Bargraph,
    Hd44780
}

#[derive(FromArgs)]
/// Workbench notifier daemon
struct ServerArgs {
    /// config file override
    #[argh(option, short = 'f')]
    cfg_file: Option<String>,
    /// do not exit if communication failure with device
    #[argh(switch, short = 'r')]
    relaxed: bool,
    /// port to bind to
    #[argh(option, short = 'p')]
    port: u16,
    /// i2c bus to connect to
    #[argh(positional)]
    dev: String,

}

enum BargraphCmd {
    SetLed(u8, u8)
}

fn main() -> Result<()> {
    let args: ServerArgs = argh::from_env();
    let dirs =
        ProjectDirs::from("", "", "wb-notifier").ok_or(eyre!("could not extract project directory"))?;

    let cfg_file = dirs.config_dir().join("workbench.json");
    let settings = Config::builder();

    let cfgs = if let Some(cfg_file_override) = args.cfg_file {
        settings
            .add_source(config::File::with_name(&cfg_file_override))
            .build()?
            .try_deserialize::<WbInfo>()?
    } else {
        settings
            .add_source(config::File::with_name(&cfg_file.to_string_lossy()))
            .build()?
            .try_deserialize::<WbInfo>()?
    };

    runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), args.port);
            let server = ServerBuilder::default().build(socket).await?;

            let (i2c_init_tx, i2c_init_rx) = sync::oneshot::channel();
            let (i2c_req_tx, mut i2c_req_rx) = sync::mpsc::channel(16);

            thread::spawn(move || {
                let mut bargraph = match i2c_init(args.dev, 0x70) {
                    Ok(b) => {
                        if let Err(_) = i2c_init_tx.send(Ok(())) {
                            return;
                        }
                        b
                    },
                    Err(e) => {
                        let _ = i2c_init_tx.send(Err(e));
                        return;
                    }
                };

                loop {
                    if let Some(r) = i2c_req_rx.blocking_recv() {
                        match r {
                            BargraphCmd::SetLed(r, c) => {
                                bargraph.set_led(r, c, true);
                            }
                        }

                    } else {
                        break;
                    }
                }
            });

            i2c_init_rx.await??;

            let mut module = RpcModule::new(());
            let req_tx = i2c_req_tx.clone();

            module.register_async_method("set_led", move|p, _| {
                let req_tx = req_tx.clone();

                async move {
                    let (r, c): (u8, u8) = p.parse()?;
                    let _ = req_tx.send(BargraphCmd::SetLed(r, c)).await;

                    Ok(())
                }
            })?;

            server.start(module)?.stopped().await;
            Ok(())
        })
}

fn i2c_init(device: String, addr: u8) -> Result<Bargraph<I2cdev>> {
    let mut i2c = I2cdev::new(device)?;
    i2c.set_slave_address(addr as u16)?;

    let mut bargraph = Bargraph::new(i2c, addr);
    bargraph.initialize()?;

    Ok(bargraph)
}

// async fn server
