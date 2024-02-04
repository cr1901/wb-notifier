use async_channel::{bounded, Sender};
use async_executor::LocalExecutor;
use async_net::{SocketAddr, UdpSocket};
use postcard_rpc::{self, Key};
use std::rc::Rc;

use wb_notifier_driver::bargraph;
use wb_notifier_driver::cmds;
use wb_notifier_driver::{self, Request};
use wb_notifier_proto::*;

use super::AsyncSend;

pub(super) mod handlers {
    use super::*;
    use background::BlinkInfo;

    pub async fn set_led<'a>(
        _ex: Rc<LocalExecutor<'_>>,
        seq_no: u32,
        key: Key,
        (sock, addr): (UdpSocket, SocketAddr),
        req_send: AsyncSend,
        SetLed { num, color }: SetLed,
    ) {
        let mut buf = vec![0u8; 1024];

        let (resp_send, resp_recv) = bounded(1);

        // For now, we give up on any send/recv/downcast/deserialize errors and
        // rely on client to time out.
        let _ = req_send
            .send((
                Request::Bargraph(cmds::Bargraph::SetLedNo { num, color }),
                resp_send,
            ))
            .await;

        let recv_res: Result<(), RequestError>;
        if let Ok(raw_res) = resp_recv.recv().await {
            recv_res = raw_res
                .map(|r| *r.downcast::<()>().unwrap())
                .map_err(|e| match e {
                    cmds::Error::Client(_) => RequestError {},
                    _ => unreachable!(),
                });
        } else {
            return
        }


        if let Ok(used) = postcard_rpc::headered::to_slice_keyed(seq_no, key, &SetLedResponse(recv_res), &mut buf) {
            let _ = sock.send_to(used, addr).await;
        }
    }

    pub async fn set_dimming<'a>(
        _ex: Rc<LocalExecutor<'_>>,
        seq_no: u32,
        key: Key,
        (sock, addr): (UdpSocket, SocketAddr),
        req_send: AsyncSend,
        dimming: SetDimming,
    ) {
        let mut buf = vec![0u8; 1024];

        let (resp_send, resp_recv) = bounded(1);

        let req = match dimming {
            SetDimming::Hi => bargraph::Dimming::BRIGHTNESS_16_16,
            SetDimming::Lo => bargraph::Dimming::BRIGHTNESS_3_16,
        };

        // For now, we give up on any send/recv/downcast/deserialize errors and
        // rely on client to time out.
        let _ = req_send
            .send((
                Request::Bargraph(cmds::Bargraph::SetBrightness { pwm: req }),
                resp_send,
            ))
            .await;

        let recv_res: Result<(), RequestError>;
        if let Ok(raw_res) = resp_recv.recv().await {
            recv_res = raw_res
                .map(|r| *r.downcast::<()>().unwrap())
                .map_err(|e| match e {
                    cmds::Error::Client(_) => RequestError {},
                    _ => unreachable!(),
                });
        } else {
            return
        }

        if let Ok(used) = postcard_rpc::headered::to_slice_keyed(seq_no, key, &SetDimmingResponse(recv_res), &mut buf) {
            let _ = sock.send_to(used, addr).await;
        }
    }

    pub async fn notify<'a>(
        _ex: Rc<LocalExecutor<'_>>,
        seq_no: u32,
        key: Key,
        (sock, addr): (UdpSocket, SocketAddr),
        req_send: AsyncSend,
        blink_send: Sender<BlinkInfo>,
        Notify { num, status }: Notify,
    ) {
        let mut buf = vec![0u8; 1024];
        let (resp_send, resp_recv) = bounded(1);

        let color = match status {
            Status::Ok => LedColor::Green,
            Status::Warning => LedColor::Yellow,
            Status::Error => LedColor::Red,
        };

        // For now, we give up on any send/recv/cl/deserialize errors and
        // rely on client to time out.
        let _ = req_send
            .send((
                Request::Bargraph(cmds::Bargraph::SetLedNo { num, color }),
                resp_send,
            ))
            .await;

        let recv_res: Result<(), RequestError>;
        if let Ok(raw_res) = resp_recv.recv().await {
            recv_res = raw_res
                .map(|r| *r.downcast::<()>().unwrap())
                .map_err(|e| match e {
                    cmds::Error::Client(_) => RequestError {},
                    _ => unreachable!(),
                });
        } else {
            return
        }

        let _ = blink_send.send(BlinkInfo::LedSet).await;
        if let Ok(used) = postcard_rpc::headered::to_slice_keyed(seq_no, key, &NotifyResponse(recv_res), &mut buf) {
            let _ = sock.send_to(used, addr).await;
        }
    }

    pub async fn ack<'a>(
        _ex: Rc<LocalExecutor<'_>>,
        seq_no: u32,
        key: Key,
        (sock, addr): (UdpSocket, SocketAddr),
        req_send: AsyncSend,
        blink_send: Sender<BlinkInfo>,
        Ack { num }: Ack,
    ) {
        let mut buf = vec![0u8; 1024];
        let (resp_send, resp_recv) = bounded(1);
        // For now, we give up on any send/recv/downcast/deserialize errors and
        // rely on client to time out.

        match num {
            Some(num) => {
                let _ = req_send
                .send((
                    Request::Bargraph(cmds::Bargraph::SetLedNo {
                        num,
                        color: LedColor::Off,
                    }),
                    resp_send,
                ))
                .await;
            },
            None => {
                let _ = req_send
                .send((
                    Request::Bargraph(cmds::Bargraph::ClearAll),
                    resp_send,
                ))
                .await;
            }
        }

        let recv_res: Result<(), RequestError>;
        if let Ok(raw_res) = resp_recv.recv().await {
            recv_res = raw_res
                .map(|r| *r.downcast::<()>().unwrap())
                .map_err(|e| match e {
                    cmds::Error::Client(_) => RequestError {},
                    _ => unreachable!(),
                });
        } else {
            return
        }

        let _ = blink_send.send(BlinkInfo::LedClear).await;
        if let Ok(used) = postcard_rpc::headered::to_slice_keyed(seq_no, key, &AckResponse(recv_res), &mut buf) {
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

    pub async fn blink<'a>(
        ex: Rc<LocalExecutor<'_>>,
        req_send: AsyncSend,
        // For now, dispatch to blink task from server without having a channel
        // to send a response.
        req_recv: Receiver<BlinkInfo>,
    ) {
        let mut state = BlinkState::Init;
        let (wait_done_send, wait_done_recv) = bounded(1);
        let (driver_resp_send, driver_resp_recv) = bounded(1);

        loop {
            let curr_task: Task<()>;
            match state {
                BlinkState::Init => {
                    curr_task = ex.spawn(future::pending());
                }
                BlinkState::Off => {
                    let _ = req_send
                        .send((
                            Request::Bargraph(cmds::Bargraph::StopBlink),
                            driver_resp_send.clone(),
                        ))
                        .await;
                    let _ = driver_resp_recv.recv().await;
                    curr_task = ex.spawn(future::pending());
                }
                BlinkState::Fast => {
                    let _ = req_send
                        .send((
                            Request::Bargraph(cmds::Bargraph::FastBlink),
                            driver_resp_send.clone(),
                        ))
                        .await;
                    let _ = driver_resp_recv.recv().await;
                    curr_task = ex.spawn(wait_then_send_done(
                        Duration::from_secs(60),
                        wait_done_send.clone(),
                    ));
                }
                BlinkState::Med => {
                    let _ = req_send
                        .send((
                            Request::Bargraph(cmds::Bargraph::MediumBlink),
                            driver_resp_send.clone(),
                        ))
                        .await;
                    let _ = driver_resp_recv.recv().await;
                    curr_task = ex.spawn(wait_then_send_done(
                        Duration::from_secs(300),
                        wait_done_send.clone(),
                    ));
                }
                BlinkState::Slow => {
                    let _ = req_send
                        .send((
                            Request::Bargraph(cmds::Bargraph::SlowBlink),
                            driver_resp_send.clone(),
                        ))
                        .await;
                    let _ = driver_resp_recv.recv().await;
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
