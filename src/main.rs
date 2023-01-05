use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::thread;
use std::time::Duration;

use argh::FromArgs;
use config::Config;
use directories::ProjectDirs;
use eyre::{bail, eyre, Result};
use ht16k33::{Dimming, Display};
use jsonrpsee::server::{RpcModule, ServerBuilder, ServerHandle};
use jsonrpsee::types::error::CallError;
use linux_embedded_hal::I2cdev;
use serde::Deserialize;
use tokio::{runtime, sync, task, time};
use wb_notifier::bargraph::{Bargraph, LedColor};

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
    relaxed: bool,
    /// port to bind to
    #[argh(option, short = 'p')]
    port: u16,
    /// i2c bus to connect to
    #[argh(positional)]
    dev: String,
}

type CmdResponse<T> = sync::oneshot::Sender<T>;

#[derive(Debug)]
enum BargraphCmd {
    Init {
        resp: CmdResponse<Result<()>>,
    },
    SetLed {
        row: u8,
        col: u8,
        resp: CmdResponse<Result<()>>,
    },
    ClearLed {
        row: u8,
        col: u8,
        resp: CmdResponse<Result<()>>,
    },
    SetLedNo {
        num: u8,
        color: LedColor,
        resp: CmdResponse<Result<()>>,
    },
    StartBlink {
        resp: CmdResponse<Result<()>>,
    },
    MediumBlink {
        resp: CmdResponse<Result<()>>,
    },
    SlowBlink {
        resp: CmdResponse<Result<()>>,
    },
    StopBlink {
        resp: CmdResponse<Result<()>>,
    },
}

#[derive(Clone, Copy)]
enum BlinkInfo {
    LedSet(u8),
    LedClear(u8),
}

#[derive(Clone, Copy)]
enum ServerState {
    Operating,
    Shutdown
}

fn main() -> Result<()> {
    let args: ServerArgs = argh::from_env();
    let dirs = ProjectDirs::from("", "", "wb-notifier")
        .ok_or(eyre!("could not extract project directory"))?;

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

            let (i2c_req_tx, i2c_req_rx) = sync::mpsc::channel(16);
            let (blink_req_tx, blink_req_rx) = sync::mpsc::channel(16);
            let (error_resp_tx, mut error_resp_rx) = sync::mpsc::channel(16);
            let (shutdown_req_tx, shutdown_req_rx) = sync::watch::channel(ServerState::Operating);
            thread::spawn(move || bargraph_driver(args.dev, 0x70, i2c_req_rx));

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
            module.register_async_method("set_led", move |p, _| {
                let req_tx = req_tx.clone();
                let blink_tx = blink_tx.clone();
                let (resp, resp_rx) = sync::oneshot::channel();

                async move {
                    let (row, col): (u8, u8) = p.parse()?;
                    req_tx.send(BargraphCmd::SetLed { row, col, resp }).await;
                    resp_rx.await.map_err(|e| Into::<anyhow::Error>::into(e))?;
                    blink_tx.send(BlinkInfo::LedSet(0)).await;

                    Ok(())
                }
            })?;

            let req_tx = i2c_req_tx.clone();
            let blink_tx = blink_req_tx.clone();
            module.register_async_method("clear_led", move |p, _| {
                let req_tx = req_tx.clone();
                let blink_tx = blink_tx.clone();
                let (resp, resp_rx) = sync::oneshot::channel();

                async move {
                    let (row, col): (u8, u8) = p.parse()?;
                    let _ = req_tx.send(BargraphCmd::ClearLed { row, col, resp }).await;
                    resp_rx.await.map_err(|e| Into::<anyhow::Error>::into(e))?;
                    blink_tx.send(BlinkInfo::LedClear(0)).await;

                    Ok(())
                }
            })?;

            let req_tx = i2c_req_tx.clone();
            let blink_tx = blink_req_tx.clone();
            module.register_async_method("set_led_no", move |p, _| {
                let req_tx = req_tx.clone();
                let blink_tx = blink_tx.clone();
                let (resp, resp_rx) = sync::oneshot::channel();

                async move {
                    let (num, color_str): (u8, String) = p.parse()?;

                    let color = match &*color_str {
                        "red" | "urgent" => LedColor::Red,
                        "yellow" | "error" => LedColor::Yellow,
                        "green" | "ok" => LedColor::Green,
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
                        .await;
                    resp_rx.await.map_err(|e| Into::<anyhow::Error>::into(e))?;

                    match color {
                        LedColor::Red | LedColor::Yellow | LedColor::Green => {
                            blink_tx.send(BlinkInfo::LedSet(num)).await
                        }
                        LedColor::Off => blink_tx.send(BlinkInfo::LedClear(num)).await,
                    };

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

            res
        })
}

// Every time we receive something, we reset the time to blink.
async fn blink_task(
    send: sync::mpsc::Sender<BargraphCmd>,
    mut recv: sync::mpsc::Receiver<BlinkInfo>,
    err: sync::mpsc::Sender<Result<()>>,
    shutdown: sync::watch::Receiver<ServerState>
) {
    loop {
        if let Some(bi) = recv.recv().await {
            if let BlinkInfo::LedSet(_) = bi {
                blink_loop(&send, &mut recv, bi).await;
            } else {
                // LED was cleared when no pending timers... nothing to do.
                continue;
            }
        } else {
            break;
        }
    }
}

// TODO: Make aware of current LEDs lit and which ones aren't to figure out
// actual time to stop blinking (use oneshots to send info about LED numbers?)
async fn blink_loop(
    send: &sync::mpsc::Sender<BargraphCmd>,
    recv: &mut sync::mpsc::Receiver<BlinkInfo>,
    _bi_init: BlinkInfo,
) {
    'blink_timer_reset: loop {
        let (resp, resp_rx) = sync::oneshot::channel();
        send.send(BargraphCmd::StartBlink { resp }).await;
        resp_rx.await;

        let sleep = time::sleep(Duration::new(60, 0));
        tokio::pin!(sleep);

        // Yikes! Refactor later...
        loop {
            tokio::select! {
                // Recv can fail... bail completely from function if so.
                r = recv.recv() => {
                    if let Some(bi) = r {
                        if let BlinkInfo::LedSet(_) = bi {
                            continue 'blink_timer_reset;
                        } else {
                            // LED cleared... cancel.
                            break 'blink_timer_reset;
                        }
                    } else {
                        return;
                    }
                },
                _ = &mut sleep => {
                    break;
                }
            }
        }

        let (resp, resp_rx) = sync::oneshot::channel();
        send.send(BargraphCmd::MediumBlink { resp }).await;
        resp_rx.await;

        let sleep = time::sleep(Duration::new(300, 0));
        tokio::pin!(sleep);

        loop {
            tokio::select! {
                // Recv can fail... bail completely from function if so.
                r = recv.recv() => {
                    if let Some(bi) = r {
                        if let BlinkInfo::LedSet(_) = bi {
                            continue 'blink_timer_reset;
                        } else {
                            // LED cleared... cancel.
                            break 'blink_timer_reset;
                        }
                    } else {
                        return;
                    }
                },
                _ = &mut sleep => {
                    break;
                }
            }
        }

        let (resp, resp_rx) = sync::oneshot::channel();
        send.send(BargraphCmd::SlowBlink { resp }).await;
        resp_rx.await;

        let sleep = time::sleep(Duration::new(900, 0));
        tokio::pin!(sleep);

        loop {
            tokio::select! {
                // Recv can fail... bail completely from function if so.
                r = recv.recv() => {
                    if let Some(bi) = r {
                        if let BlinkInfo::LedSet(_) = bi {
                            continue 'blink_timer_reset;
                        } else {
                            // LED cleared... cancel.
                            break 'blink_timer_reset;
                        }
                    } else {
                        return;
                    }
                },
                _ = &mut sleep => {
                    break 'blink_timer_reset;
                }
            }
        }
    }

    let (resp, resp_rx) = sync::oneshot::channel();
    send.send(BargraphCmd::StopBlink { resp }).await;
    resp_rx.await;
}

fn bargraph_driver(device: String, addr: u8, mut recv: sync::mpsc::Receiver<BargraphCmd>) {
    let mut bargraph = if let Some(cmd) = recv.blocking_recv() {
        if let BargraphCmd::Init { resp } = cmd {
            match i2c_init(device, addr) {
                Ok(b) => {
                    if let Err(_) = resp.send(Ok(())) {
                        return;
                    }
                    b
                }
                Err(e) => {
                    let _ = resp.send(Err(e));
                    return;
                }
            }
        } else {
            return;
        }
    } else {
        return;
    };

    loop {
        if let Some(r) = recv.blocking_recv() {
            match r {
                BargraphCmd::SetLed { row, col, resp } => {
                    let res = bargraph.set_led(row, col, true);
                    resp.send(res.map_err(|e| e.into()));
                }
                BargraphCmd::ClearLed { row, col, resp } => {
                    let res = bargraph.set_led(row, col, false);
                    resp.send(res.map_err(|e| e.into()));
                }
                BargraphCmd::SetLedNo { num, color, resp } => {
                    let res = bargraph.set_led_no(num, color);
                    resp.send(res.map_err(|e| e.into()));
                }
                BargraphCmd::StartBlink { resp } => {
                    let res = bargraph.set_display(Display::TWO_HZ);
                    resp.send(res.map_err(|e| e.into()));
                }
                BargraphCmd::MediumBlink { resp } => {
                    let res = bargraph.set_display(Display::ONE_HZ);
                    resp.send(res.map_err(|e| e.into()));
                }
                BargraphCmd::SlowBlink { resp } => {
                    let res = bargraph.set_display(Display::HALF_HZ);
                    resp.send(res.map_err(|e| e.into()));
                }
                BargraphCmd::StopBlink { resp } => {
                    let res = bargraph.set_display(Display::ON);
                    resp.send(res.map_err(|e| e.into()));
                }
                BargraphCmd::Init { resp, .. } => {
                    resp.send(Ok(()));
                }
            }
        } else {
            // Only happens if the req_rx channel has closed.
            break;
        }
    }
}

fn i2c_init(device: String, addr: u8) -> Result<Bargraph<I2cdev>> {
    let mut i2c = I2cdev::new(device)?;
    i2c.set_slave_address(addr as u16)?;

    let mut bargraph = Bargraph::new(i2c, addr);
    bargraph.initialize()?;
    bargraph.set_dimming(Dimming::BRIGHTNESS_3_16)?;

    Ok(bargraph)
}

// async fn server
