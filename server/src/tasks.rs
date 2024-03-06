use async_channel::{bounded, Sender};
use async_executor::LocalExecutor;
use async_lock::Mutex;
use async_net::{SocketAddr, UdpSocket};
use blocking::unblock;
use embedded_hal::blocking::i2c::{Write, WriteRead};
use postcard_rpc::{self, Key};
use std::rc::Rc;
use std::sync::Arc;

use wb_notifier_driver::bargraph;
use wb_notifier_driver::lcd;

use wb_notifier_driver;
use wb_notifier_proto::*;

pub(super) mod handlers {
    use super::*;
    use background::BlinkInfo;
    use embedded_hal::blocking::delay::{DelayMs, DelayUs};

    pub async fn set_led<'a, I2C, E>(
        _ex: Rc<LocalExecutor<'_>>,
        seq_no: u32,
        key: Key,
        (sock, addr): (UdpSocket, SocketAddr),
        bg: Arc<Mutex<bargraph::Bargraph<I2C>>>,
        SetLed { num, color }: SetLed,
    ) where
        I2C: Send + Write<Error = E> + WriteRead<Error = E> + 'static,
        E: Send + 'static,
    {
        let mut buf = vec![0u8; 1024];

        // For now, we give up on any send/recv/downcast/deserialize errors and
        // rely on client to time out.
        let bg = bg.clone();
        let res = unblock(move || bg.lock_arc_blocking().set_led_no(num, color)).await;

        let resp_res = if res.is_ok() {
            SetLedResponse(Ok(()))
        } else {
            SetLedResponse(Err(RequestError {}))
        };

        if let Ok(used) = postcard_rpc::headered::to_slice_keyed(seq_no, key, &resp_res, &mut buf) {
            let _ = sock.send_to(used, addr).await;
        }
    }

    pub async fn set_dimming<'a, I2C, E>(
        _ex: Rc<LocalExecutor<'_>>,
        seq_no: u32,
        key: Key,
        (sock, addr): (UdpSocket, SocketAddr),
        bg: Arc<Mutex<bargraph::Bargraph<I2C>>>,
        dimming: SetDimming,
    ) where
        I2C: Send + Write<Error = E> + WriteRead<Error = E> + 'static,
        E: Send + 'static,
    {
        let mut buf = vec![0u8; 1024];

        let req = match dimming {
            SetDimming::Hi => bargraph::Dimming::BRIGHTNESS_16_16,
            SetDimming::Lo => bargraph::Dimming::BRIGHTNESS_3_16,
        };

        // For now, we give up on any send/recv/downcast/deserialize errors and
        // rely on client to time out.
        let bg = bg.clone();
        let res = unblock(move || bg.lock_arc_blocking().set_dimming(req)).await;

        let resp_res = if res.is_ok() {
            SetDimmingResponse(Ok(()))
        } else {
            SetDimmingResponse(Err(RequestError {}))
        };

        if let Ok(used) = postcard_rpc::headered::to_slice_keyed(seq_no, key, &resp_res, &mut buf) {
            let _ = sock.send_to(used, addr).await;
        }
    }

    pub async fn notify<'a, I2C, E>(
        _ex: Rc<LocalExecutor<'_>>,
        seq_no: u32,
        key: Key,
        (sock, addr): (UdpSocket, SocketAddr),
        blink_send: Sender<BlinkInfo>,
        bg: Arc<Mutex<bargraph::Bargraph<I2C>>>,
        Notify { num, status }: Notify,
    ) where
        I2C: Send + Write<Error = E> + WriteRead<Error = E> + 'static,
        E: Send + 'static,
    {
        let mut buf = vec![0u8; 1024];

        let color = match status {
            Status::Ok => LedColor::Green,
            Status::Warning => LedColor::Yellow,
            Status::Error => LedColor::Red,
        };

        // For now, we give up on any send/recv/cl/deserialize errors and
        // rely on client to time out.
        let bg = bg.clone();
        let res = unblock(move || bg.lock_arc_blocking().set_led_no(num, color)).await;

        let resp_res = if res.is_ok() {
            NotifyResponse(Ok(()))
        } else {
            NotifyResponse(Err(RequestError {}))
        };

        let _ = blink_send.send(BlinkInfo::LedSet).await;
        if let Ok(used) = postcard_rpc::headered::to_slice_keyed(seq_no, key, &resp_res, &mut buf) {
            let _ = sock.send_to(used, addr).await;
        }
    }

    pub async fn ack<'a, I2C, E>(
        _ex: Rc<LocalExecutor<'_>>,
        seq_no: u32,
        key: Key,
        (sock, addr): (UdpSocket, SocketAddr),
        blink_send: Sender<BlinkInfo>,
        bg: Arc<Mutex<bargraph::Bargraph<I2C>>>,
        Ack { num }: Ack,
    ) where
        I2C: Send + Write<Error = E> + WriteRead<Error = E> + 'static,
        E: Send + 'static,
    {
        let mut buf = vec![0u8; 1024];
        // For now, we give up on any send/recv/downcast/deserialize errors and
        // rely on client to time out.

        let resp_res;
        match num {
            Some(num) => {
                let bg = bg.clone();
                let res =
                    unblock(move || bg.lock_arc_blocking().set_led_no(num, LedColor::Off)).await;

                resp_res = if res.is_ok() {
                    AckResponse(Ok(()))
                } else {
                    AckResponse(Err(RequestError {}))
                };
            }
            None => {
                let bg = bg.clone();
                let res = unblock(move || bg.lock_arc_blocking().clear_all()).await;

                resp_res = if res.is_ok() {
                    AckResponse(Ok(()))
                } else {
                    AckResponse(Err(RequestError {}))
                };
            }
        }

        let _ = blink_send.send(BlinkInfo::LedClear).await;
        if let Ok(used) = postcard_rpc::headered::to_slice_keyed(seq_no, key, &resp_res, &mut buf) {
            let _ = sock.send_to(used, addr).await;
        }
    }

    pub async fn set_backlight<'a, I2C, E, D>(
        _ex: Rc<LocalExecutor<'_>>,
        seq_no: u32,
        key: Key,
        (sock, addr): (UdpSocket, SocketAddr),
        lcd: Arc<Mutex<lcd::Lcd<I2C, D>>>,
        backlight: SetBacklight,
    ) where
        I2C: Send + Write<Error = E> + WriteRead<Error = E> + 'static,
        E: Send + 'static,
        D: DelayMs<u8> + DelayUs<u16> + Send + 'static
    {
        let mut buf = vec![0u8; 1024];
        // For now, we give up on any send/recv/downcast/deserialize errors and
        // rely on client to time out.

        let res = unblock(move || {
            let mut lcd = lcd.lock_arc_blocking();
            lcd.set_backlight(backlight)
        }).await;
        let resp_res = if res.is_ok() {
            SetBacklightResponse(Ok(()))
        } else {
            SetBacklightResponse(Err(RequestError {}))
        };

        if let Ok(used) = postcard_rpc::headered::to_slice_keyed(seq_no, key, &resp_res, &mut buf) {
            let _ = sock.send_to(used, addr).await;
        }
    }

    pub async fn echo<'a>(
        _ex: Rc<LocalExecutor<'_>>,
        seq_no: u32,
        key: Key,
        (sock, addr): (UdpSocket, SocketAddr),
        msg: String,
    ) {
        let resp = EchoResponse(msg.to_uppercase());
        let mut buf = vec![0u8; 1024];

        if let Ok(used) = postcard_rpc::headered::to_slice_keyed(seq_no, key, &resp, &mut buf) {
            let _ = sock.send_to(used, addr).await;
        }
    }
}

pub(super) mod background {
    use std::time::Duration;

    use super::*;
    use async_channel::{Receiver, TryRecvError};
    use async_executor::Task;
    use async_io::Timer;
    use futures_lite::future;
    use futures_lite::{Future, FutureExt};

    enum FinishedFirst<U, T> {
        Us(U),
        Them(T),
    }

    async fn select<FU, FT, U, T>(us: FU, them: FT) -> FinishedFirst<U, T>
    where
        FU: Future<Output = U>,
        FT: Future<Output = T>,
    {
        let us = async {
            let res = us.await;
            FinishedFirst::Us(res)
        };

        let them = async {
            let res = them.await;
            FinishedFirst::Them(res)
        };

        us.or(them).await
    }

    enum BlinkState {
        Init,
        Off,
        Fast,
        Med,
        Slow,
    }

    #[derive(Clone, Copy, Debug)]
    pub enum BlinkInfo {
        LedSet,
        LedClear,
    }

    pub async fn blink<'a, I2C, E>(
        ex: Rc<LocalExecutor<'_>>,
        bg: Arc<Mutex<bargraph::Bargraph<I2C>>>,
        // For now, dispatch to blink task from server without having a channel
        // to send a response.
        req_recv: Receiver<BlinkInfo>,
    ) where
        I2C: Send + Write<Error = E> + WriteRead<Error = E> + 'static,
        E: Send + 'static,
    {
        let mut state = BlinkState::Init;
        let (wait_done_send, wait_done_recv) = bounded(1);
        // let (driver_resp_send, driver_resp_recv) = bounded(1);

        loop {
            let curr_task: Task<()>;
            match state {
                BlinkState::Init => {
                    curr_task = ex.spawn(future::pending());
                }
                BlinkState::Off => {
                    let bg = bg.clone();
                    unblock(move || {
                        let _ = bg.lock_arc_blocking().set_display(bargraph::Display::ON);
                    })
                    .await;
                    curr_task = ex.spawn(future::pending());
                }
                BlinkState::Fast => {
                    let bg = bg.clone();
                    unblock(move || {
                        let _ = bg
                            .lock_arc_blocking()
                            .set_display(bargraph::Display::TWO_HZ);
                    })
                    .await;
                    curr_task = ex.spawn(wait_then_send_done(
                        Duration::from_secs(60),
                        wait_done_send.clone(),
                    ));
                }
                BlinkState::Med => {
                    let bg = bg.clone();
                    unblock(move || {
                        let _ = bg
                            .lock_arc_blocking()
                            .set_display(bargraph::Display::ONE_HZ);
                    })
                    .await;
                    curr_task = ex.spawn(wait_then_send_done(
                        Duration::from_secs(300),
                        wait_done_send.clone(),
                    ));
                }
                BlinkState::Slow => {
                    let bg = bg.clone();
                    unblock(move || {
                        let _ = bg
                            .lock_arc_blocking()
                            .set_display(bargraph::Display::HALF_HZ);
                    })
                    .await;
                    curr_task = ex.spawn(wait_then_send_done(
                        Duration::from_secs(900),
                        wait_done_send.clone(),
                    ));
                }
            }

            match select(req_recv.recv(), wait_done_recv.clone().recv()).await {
                FinishedFirst::Them(_) => match state {
                    BlinkState::Init | BlinkState::Slow => {
                        state = BlinkState::Off;
                    }
                    BlinkState::Off => {
                        state = BlinkState::Fast;
                    }
                    BlinkState::Fast => {
                        state = BlinkState::Med;
                    }
                    BlinkState::Med => {
                        state = BlinkState::Slow;
                    }
                },
                FinishedFirst::Us(Ok(led)) => {
                    curr_task.cancel().await;
                    // Drain the channel to ensure it's empty for the next time
                    // we run the task.
                    if let Err(TryRecvError::Closed) = wait_done_recv.try_recv() {
                        break;
                    }

                    match led {
                        BlinkInfo::LedSet => state = BlinkState::Fast,
                        BlinkInfo::LedClear => state = BlinkState::Off,
                    }
                }
                FinishedFirst::Us(Err(_)) => {
                    curr_task.cancel().await;
                    break;
                }
            }
        }
    }

    async fn wait_then_send_done(amt: Duration, done: Sender<()>) {
        Timer::after(amt).await;
        // This unwrap should never fire, because only one of these tasks
        // active at any given time.
        // If we cancel the task, we check the output and drain the channel
        // to ensure it's empty for the next time we run the task.
        done.send(()).await.unwrap();
    }
}
