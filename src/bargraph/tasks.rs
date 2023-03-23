use std::{error, fmt, time::Duration};

use eyre::Result;
use ht16k33::{Dimming, Display};
use linux_embedded_hal::I2cdev;
use tokio::{sync, time};

use super::driver::{Bargraph, LedColor};
use crate::server::ServerState;

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
    SetBrightness {
        pwm: Dimming,
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

pub struct BlinkTask {
    cmd: sync::mpsc::Sender<BargraphCmd>,
    info: sync::mpsc::Receiver<BlinkInfo>,
    err: sync::mpsc::Sender<Result<()>>,
    shutdown: sync::watch::Receiver<ServerState>,
    _shutdown_complete: sync::mpsc::Sender<()>,
}

#[derive(Clone, Copy, Debug)]
pub enum BlinkInfo {
    LedSet(u8),
    LedClear(u8),
}

// Every time we receive something, we reset the time to blink.
pub async fn blink_task(
    send: sync::mpsc::Sender<BargraphCmd>,
    mut recv: sync::mpsc::Receiver<BlinkInfo>,
    err: sync::mpsc::Sender<Result<()>>,
    shutdown: sync::watch::Receiver<ServerState>,
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

pub struct BlockingEventLoop {
    cmd: sync::mpsc::Receiver<BargraphCmd>,
    err: sync::mpsc::Sender<Result<()>>,
}

#[derive(Copy, Clone, Debug)]
enum EventLoopError {
    CmdChannelClosed,
    InitNotCalledFirst,
    InitFailure,
}

impl fmt::Display for EventLoopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventLoopError::CmdChannelClosed => write!(f, "cmd channel was closed"),
            EventLoopError::InitNotCalledFirst => {
                write!(f, "event loop did not receive init command first")
            }
            EventLoopError::InitFailure => write!(f, "init failure in driver"),
        }
    }
}

impl error::Error for EventLoopError {}

impl BlockingEventLoop {
    pub fn new(
        cmd: sync::mpsc::Receiver<BargraphCmd>,
        err: sync::mpsc::Sender<Result<()>>,
    ) -> Self {
        BlockingEventLoop { cmd, err }
    }

    pub fn run(&mut self, device: String, addr: u8) {
        let mut bargraph = match self.cmd.blocking_recv() {
            Some(BargraphCmd::Init { resp }) => {
                match Self::init(device, addr) {
                    Ok(b) => {
                        if let Err(e) = resp.send(Ok(())) {
                            // If err channel is closed, we're already packing
                            // it in, so ignore.
                            let _ = self.err.blocking_send(e);
                            return;
                        }
                        b
                    }
                    Err(e) => {
                        let _ = resp.send(Err(e));
                        // Initialization failure.
                        let _ = self
                            .err
                            .blocking_send(Err(EventLoopError::InitFailure.into()));
                        return;
                    }
                }
            }
            Some(_) => {
                // Synchronization failure.
                let _ = self
                    .err
                    .blocking_send(Err(EventLoopError::InitNotCalledFirst.into()));
                return;
            }
            None => {
                // If cmd channel is closed, we're already packing
                // it in, so ignore.
                let _ = self
                    .err
                    .blocking_send(Err(EventLoopError::CmdChannelClosed.into()));
                return;
            }
        };

        loop {
            if let Some(r) = self.cmd.blocking_recv() {
                match r {
                    BargraphCmd::SetLed { row, col, resp } => {
                        let res = bargraph.set_led(row, col, true);
                        if let Err(_) = resp.send(res.map_err(|e| e.into())) {
                            return;
                        }
                    }
                    BargraphCmd::ClearLed { row, col, resp } => {
                        let res = bargraph.set_led(row, col, false);
                        if let Err(_) = resp.send(res.map_err(|e| e.into())) {
                            return;
                        }
                    }
                    BargraphCmd::SetLedNo { num, color, resp } => {
                        let res = bargraph.set_led_no(num, color);
                        if let Err(_) = resp.send(res.map_err(|e| e.into())) {
                            return;
                        }
                    }
                    BargraphCmd::SetBrightness { pwm, resp } => {
                        let res = bargraph.set_dimming(pwm);
                        if let Err(_) = resp.send(res.map_err(|e| e.into())) {
                            return;
                        }
                    }
                    BargraphCmd::StartBlink { resp } => {
                        let res = bargraph.set_display(Display::TWO_HZ);
                        if let Err(_) = resp.send(res.map_err(|e| e.into())) {
                            return;
                        }
                    }
                    BargraphCmd::MediumBlink { resp } => {
                        let res = bargraph.set_display(Display::ONE_HZ);
                        if let Err(_) = resp.send(res.map_err(|e| e.into())) {
                            return;
                        }
                    }
                    BargraphCmd::SlowBlink { resp } => {
                        let res = bargraph.set_display(Display::HALF_HZ);
                        if let Err(_) = resp.send(res.map_err(|e| e.into())) {
                            return;
                        }
                    }
                    BargraphCmd::StopBlink { resp } => {
                        let res = bargraph.set_display(Display::ON);
                        if let Err(_) = resp.send(res.map_err(|e| e.into())) {
                            return;
                        }
                    }
                    BargraphCmd::Init { resp, .. } => {
                        let res = bargraph.initialize();
                        if let Err(_) = resp.send(res.map_err(|e| e.into())) {
                            return;
                        }
                    }
                }
            } else {
                // Only happens if the req_rx channel has closed.
                break;
            }
        }
    }

    fn init(device: String, addr: u8) -> Result<Bargraph<I2cdev>> {
        let mut i2c = I2cdev::new(device)?;
        i2c.set_slave_address(addr as u16)?;

        let mut bargraph = Bargraph::new(i2c, addr);
        bargraph.initialize()?;
        bargraph.set_dimming(Dimming::BRIGHTNESS_3_16)?;

        Ok(bargraph)
    }
}
