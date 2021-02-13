use std::borrow::{Borrow, BorrowMut};
use std::sync::atomic::{AtomicI32, Ordering};
use std::thread;
use std::time::Duration;

use async_std::future;
use async_std::io;
use async_std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use async_std::sync::{Arc, Mutex, RwLock};
use async_std::task;
use clap::*;
use futures::executor::block_on;
use log::{debug, error, info, warn, LevelFilter};

use crate::logger;
use crate::logger::LOGGER;
use std::str::FromStr;

const BUFFER_SIZE: usize = 2048;

pub struct UpstreamInfo {
    id: u32,
    downstream_addr: SocketAddr,
    upstream_socket: UdpSocket,
    last_active_ts: u64,
}

async fn info_recv(
    info: Arc<UpstreamInfo>,
    infos: Arc<RwLock<Vec<Arc<UpstreamInfo>>>>,
    downstream_socket: Arc<UdpSocket>,
    mode: Mode,
    prepend_size: usize,
    timeout: Duration,
) {
    let (recv_bg_pos, send_bg_pos) = mode.build_begin_position(prepend_size);
    let mut buf = [0u8; BUFFER_SIZE];
    loop {
        match io::timeout(
            timeout,
            info.upstream_socket.recv_from(&mut buf[recv_bg_pos..]),
        )
        .await
        {
            Err(e) => {
                warn!("found receive from downstream #{} error: {}", info.id, e);
                let mut w = infos.write().await;
                let pos = w
                    .iter()
                    .position(|i| i.id == info.id)
                    .expect("should find socket info");
                w.remove(pos);
                break;
            }
            Ok((rev_size, _)) => {
                for i in 0..recv_bg_pos {
                    buf[i] = buf[recv_bg_pos + rev_size - i - 1]
                }
                downstream_socket
                    .send_to(
                        &buf[send_bg_pos..recv_bg_pos + rev_size],
                        &info.downstream_addr,
                    )
                    .await;
                for i in 0..recv_bg_pos {
                    buf[i] = buf[recv_bg_pos + rev_size - i - 1]
                }
                downstream_socket
                    .send_to(
                        &buf[send_bg_pos..recv_bg_pos + rev_size],
                        &info.downstream_addr,
                    )
                    .await;
            }
        }
    }
}

pub enum Mode {
    Encode,
    Decode,
}

impl Mode {
    pub fn build_begin_position(&self, prepend_size: usize) -> (usize, usize) {
        match self {
            Mode::Decode => (prepend_size, 0),
            Mode::Encode => (0, prepend_size),
        }
    }
    pub fn reverse(&self) -> Mode {
        match self {
            Mode::Decode => Mode::Encode,
            Mode::Encode => Mode::Decode,
        }
    }
}

impl std::str::FromStr for Mode {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase() {
            s if s == "encode" => Ok(Mode::Encode),
            s if s == "decode" => Ok(Mode::Encode),
            _ => Err(Error::with_description(
                "invalid mode",
                ErrorKind::InvalidValue,
            )),
        }
    }
}

pub struct UdpPrepend {
    prepend_size: usize,
    mode: Mode,
    bind: SocketAddr,
    upstream: SocketAddr,
    upstream_timeout: Duration,
    downstream_socket: Arc<UdpSocket>,
    upstream_infos: Arc<RwLock<Vec<Arc<UpstreamInfo>>>>,
}

impl UdpPrepend {
    pub async fn new(
        prepend_size: usize,
        mode: Mode,
        bind: SocketAddr,
        upstream: SocketAddr,
        upstream_timeout: Duration,
    ) -> UdpPrepend {
        let downstream_socket = UdpSocket::bind(bind)
            .await
            .expect("failed to bind local socket");
        UdpPrepend {
            prepend_size,
            mode,
            bind,
            upstream,
            upstream_timeout,
            downstream_socket: Arc::new(downstream_socket),
            upstream_infos: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn start_forward(&self) {
        let mut buf = [0u8; BUFFER_SIZE];
        let (recv_bg_pos, send_bg_pos) = self.mode.build_begin_position(self.prepend_size);
        let mut info_id_counter: u32 = 0;
        loop {
            let (rev_size, downstream_addr) = self
                .downstream_socket
                .recv_from(&mut buf[recv_bg_pos..])
                .await
                .expect("should get data success");
            for i in 0..recv_bg_pos {
                buf[i] = buf[recv_bg_pos + rev_size - i - 1]
            }

            let existing_info = {
                let read = &*self.upstream_infos.read().await;
                read.iter()
                    .find(|&info| info.downstream_addr.eq(&downstream_addr))
                    .map(|i| i.clone())
            };

            let info = match existing_info {
                Some(info) => info,
                None => {
                    let upstream_socket = UdpSocket::bind("0.0.0.0:0")
                        .await
                        .expect("failed to bind addr for upstream conn");
                    let info = Arc::new(UpstreamInfo {
                        id: info_id_counter,
                        downstream_addr,
                        upstream_socket,
                        last_active_ts: 0,
                    });
                    info_id_counter += 1;
                    task::spawn(info_recv(
                        info.clone(),
                        self.upstream_infos.clone(),
                        self.downstream_socket.clone(),
                        self.mode.reverse(),
                        self.prepend_size,
                        self.upstream_timeout,
                    ));
                    let mut write = &mut *self.upstream_infos.write().await;
                    write.push(info.clone());
                    info
                }
            };

            info.upstream_socket
                .send_to(&buf[send_bg_pos..recv_bg_pos + rev_size], &self.upstream)
                .await;
        }
    }
}
