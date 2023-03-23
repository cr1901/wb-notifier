#[cfg(feature = "cli")]
compile_error!("please build the daemon without the cli feature");

use std::error::Error as StdError;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::thread;

use anyhow::anyhow;
use argh::FromArgs;
use config::Config;
use directories::ProjectDirs;
use eyre::{eyre, Report, Result};
use ht16k33::Dimming;
use jsonrpsee::server::{RpcModule, ServerBuilder};
use jsonrpsee::types::error::CallError;
use serde::Deserialize;
use tokio::{runtime, sync, task};
use wb_notifier::bargraph::driver::LedColor;
use wb_notifier::bargraph::{
    self,
    tasks::{blink_task, BargraphCmd, BlinkInfo},
};
use wb_notifier::server::ServerState;

#[derive(Deserialize, Hash)]
struct WbInfo {
    devices: Vec<Device>,
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
    Hd44780,
}

#[derive(FromArgs)]
/// Workbench notifier daemon
struct ServerArgs {
    /// config file override
    #[argh(option, short = 'f')]
    cfg_file: Option<String>,
    /// do not exit if communication failure with device
    #[argh(switch, short = 'r')]
    #[allow(unused)]
    relaxed: bool,
    /// port to bind to
    #[argh(option, short = 'p')]
    port: u16,
    /// i2c bus to connect to
    #[argh(positional)]
    dev: String,
}

fn report_to_anyhow(r: Report) -> anyhow::Error {
    let boxed: Box<dyn StdError + Send + Sync + 'static> = Box::from(r);
    anyhow!(boxed)
}

fn main() -> Result<()> {
    let args: ServerArgs = argh::from_env();
    let dirs = ProjectDirs::from("", "", "wb-notifier")
        .ok_or(eyre!("could not extract project directory"))?;

    let cfg_file = dirs.config_dir().join("workbench.json");
    let settings = Config::builder();

    #[allow(unused)]
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

            let (i2c_req_tx, i2c_req_rx) = sync::mpsc::channel(16);
            let (blink_req_tx, blink_req_rx) = sync::mpsc::channel(16);
            let (error_resp_tx, mut error_resp_rx) = sync::mpsc::channel(16);
            let (shutdown_req_tx, shutdown_req_rx) = sync::watch::channel(ServerState::Operating);
            let (shutdown_complete_tx, mut shutdown_complete_rx) = sync::mpsc::channel::<()>(1);

            let mut bargraph_evs =
                bargraph::tasks::BlockingEventLoop::new(i2c_req_rx, error_resp_tx.clone());
            thread::spawn(move || bargraph_evs.run(args.dev, 0x70));

            let req_tx = i2c_req_tx.clone();
            let shutdown_rx = shutdown_req_rx.clone();
            let err_tx = error_resp_tx.clone();
            task::spawn(async move {
                blink_task(req_tx, blink_req_rx, err_tx, shutdown_rx).await;
            });

            let (resp, resp_rx) = sync::oneshot::channel();
            i2c_req_tx.send(BargraphCmd::Init { resp }).await?;
            resp_rx.await??;

            let mut module = RpcModule::new(());

            let req_tx = i2c_req_tx.clone();
            let blink_tx = blink_req_tx.clone();
            let shutdown_tx = shutdown_complete_tx.clone();
            module.register_async_method("set_led", move |p, _| {
                let req_tx = req_tx.clone();
                let blink_tx = blink_tx.clone();
                let shutdown_tx = shutdown_tx.clone();
                let (resp, resp_rx) = sync::oneshot::channel();

                async move {
                    let _ = shutdown_tx.clone();
                    let (row, col): (u8, u8) = p.parse()?;
                    req_tx.send(BargraphCmd::SetLed { row, col, resp })
                          .await
                          .map_err(anyhow::Error::from)?;
                    resp_rx.await
                           .map_err(anyhow::Error::from)?
                           .map_err(report_to_anyhow)?;
                    blink_tx.send(BlinkInfo::LedSet(0))
                            .await
                            .map_err(anyhow::Error::from)?;

                    Ok(())
                }
            })?;

            let req_tx = i2c_req_tx.clone();
            let blink_tx = blink_req_tx.clone();
            let shutdown_tx = shutdown_complete_tx.clone();
            module.register_async_method("clear_led", move |p, _| {
                let req_tx = req_tx.clone();
                let blink_tx = blink_tx.clone();
                let shutdown_tx = shutdown_tx.clone();
                let (resp, resp_rx) = sync::oneshot::channel();

                async move {
                    let _ = shutdown_tx.clone();
                    let (row, col): (u8, u8) = p.parse()?;
                    req_tx.send(BargraphCmd::ClearLed { row, col, resp }).await.map_err(anyhow::Error::from)?;
                    resp_rx.await.map_err(anyhow::Error::from)?.map_err(report_to_anyhow)?;
                    blink_tx.send(BlinkInfo::LedClear(0)).await.map_err(anyhow::Error::from)?;

                    Ok(())
                }
            })?;

            let req_tx = i2c_req_tx.clone();
            let blink_tx = blink_req_tx.clone();
            let shutdown_tx = shutdown_complete_tx.clone();
            module.register_async_method("set_led_no", move |p, _| {
                let req_tx = req_tx.clone();
                let blink_tx = blink_tx.clone();
                let shutdown_tx = shutdown_tx.clone();
                let (resp, resp_rx) = sync::oneshot::channel();

                async move {
                    let _ = shutdown_tx.clone();
                    let (num, color_str): (u8, String) = p.parse()?;

                    let color = match &*color_str {
                        "red" | "urgent" => LedColor::Red,
                        "yellow" | "error" => LedColor::Yellow,
                        "on" | "green" | "ok" => LedColor::Green,
                        "off" | "ack" | "clear" => LedColor::Off,
                        _ => {
                            return Err(CallError::InvalidParams(anyhow::anyhow!(
                                "unexpected led color string"
                            ))
                            .into())
                        }
                    };

                    req_tx
                        .send(BargraphCmd::SetLedNo { num, color, resp })
                        .await
                        .map_err(anyhow::Error::from)?;
                    resp_rx.await.map_err(anyhow::Error::from)?.map_err(report_to_anyhow)?;

                    match color {
                        LedColor::Red | LedColor::Yellow | LedColor::Green => {
                            blink_tx.send(BlinkInfo::LedSet(num)).await.map_err(anyhow::Error::from)?
                        }
                        LedColor::Off => blink_tx.send(BlinkInfo::LedClear(num)).await.map_err(anyhow::Error::from)?,
                    };

                    Ok(())
                }
            })?;

            let req_tx = i2c_req_tx.clone();
            let blink_tx = blink_req_tx.clone();
            let shutdown_tx = shutdown_complete_tx.clone();
            module.register_async_method("set_brightness", move |p, _| {
                let req_tx = req_tx.clone();
                let blink_tx = blink_tx.clone();
                let shutdown_tx = shutdown_tx.clone();
                let (resp, resp_rx) = sync::oneshot::channel();

                async move {
                    let _ = shutdown_tx.clone();
                    let pwm = match p.parse()? {
                        1 => Dimming::BRIGHTNESS_1_16,
                        2 => Dimming::BRIGHTNESS_2_16,
                        3 => Dimming::BRIGHTNESS_3_16,
                        4 => Dimming::BRIGHTNESS_4_16,
                        5 => Dimming::BRIGHTNESS_5_16,
                        6 => Dimming::BRIGHTNESS_6_16,
                        7 => Dimming::BRIGHTNESS_7_16,
                        8 => Dimming::BRIGHTNESS_8_16,
                        9 => Dimming::BRIGHTNESS_9_16,
                        10 => Dimming::BRIGHTNESS_10_16,
                        11 => Dimming::BRIGHTNESS_11_16,
                        12 => Dimming::BRIGHTNESS_12_16,
                        13 => Dimming::BRIGHTNESS_13_16,
                        14 => Dimming::BRIGHTNESS_14_16,
                        15 => Dimming::BRIGHTNESS_15_16,
                        16 => Dimming::BRIGHTNESS_16_16,
                        e => {
                            return Err(CallError::InvalidParams(anyhow::anyhow!(
                                "expected integer between 1 and 16, got {}",
                                e
                            ))
                            .into())
                        }
                    };

                    req_tx.send(BargraphCmd::SetBrightness { pwm, resp }).await.map_err(anyhow::Error::from)?;
                    resp_rx.await.map_err(anyhow::Error::from)?.map_err(report_to_anyhow)?;
                    blink_tx.send(BlinkInfo::LedClear(0)).await.map_err(anyhow::Error::from)?;

                    Ok(())
                }
            })?;

            let req_tx = i2c_req_tx.clone();
            let blink_tx = blink_req_tx.clone();
            let shutdown_tx = shutdown_complete_tx.clone();
            module.register_async_method("reset", move |_, _| {
                let req_tx = req_tx.clone();
                let blink_tx = blink_tx.clone();
                let shutdown_tx = shutdown_tx.clone();
                let (resp, resp_rx) = sync::oneshot::channel();

                async move {
                    let _ = shutdown_tx.clone();
                    let _ = req_tx.send(BargraphCmd::Init { resp }).await;
                    resp_rx.await.map_err(anyhow::Error::from)?.map_err(report_to_anyhow)?;
                    blink_tx.send(BlinkInfo::LedClear(0)).await.map_err(anyhow::Error::from)?;

                    Ok(())
                }
            })?;

            let res = tokio::select! {
                _ = server.start(module)?.stopped() => {
                    // If error, then everything has already shut down.
                    let _ = shutdown_req_tx.send(ServerState::Shutdown);
                    Ok(())
                },
                e = error_resp_rx.recv() => {
                    // If error, then everything has already shut down.
                    let _ = shutdown_req_tx.send(ServerState::Shutdown);
                    e.unwrap_or(Err(eyre!("error channel shutdown before receiving value")))
                }
            };

            // We want the blocking event loop to exit too; blocking_recv
            // will return None to signal no more senders when all tasks have
            // dropped.
            drop(i2c_req_tx);

            // Wait for everything to shut down.
            shutdown_complete_rx.recv().await;
            res
        })
}
