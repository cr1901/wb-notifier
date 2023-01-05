use std::time::Duration;

use eyre::Result;
use ht16k33::{Dimming, Display};
use linux_embedded_hal::I2cdev;
use tokio::{sync, time};

use crate::server::ServerState;
use super::driver::{Bargraph, LedColor};

type CmdResponse<T> = sync::oneshot::Sender<T>;

#[derive(Debug)]
pub enum BargraphCmd {
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
pub enum BlinkInfo {
    LedSet(u8),
    LedClear(u8),
}

// Every time we receive something, we reset the time to blink.
pub async fn blink_task(
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
pub async fn blink_loop(
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

pub fn bargraph_driver(device: String, addr: u8, mut recv: sync::mpsc::Receiver<BargraphCmd>) {
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
