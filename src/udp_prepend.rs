use std::time::Duration;

use async_std::io;
use async_std::net::{SocketAddr, UdpSocket};
use async_std::sync::{Arc, RwLock};
use async_std::task;
use clap::*;
use log::{error, warn};

const BUFFER_SIZE: usize = 65 << 10; // 65 KB, larger than the max possible udp package

pub struct UpstreamInfo {
    id: u32,
    downstream_addr: SocketAddr,
    upstream_socket: UdpSocket,
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
                error!(
                    "error while receiving from downstream #{} error: {}",
                    info.id, e
                );
                break;
            }
            Ok((rev_size, _)) => {
                for i in 0..recv_bg_pos {
                    buf[i] = buf[recv_bg_pos + rev_size - i - 1]
                }
                match downstream_socket
                    .send_to(
                        &buf[send_bg_pos..recv_bg_pos + rev_size],
                        &info.downstream_addr,
                    )
                    .await
                {
                    Err(e) => {
                        error!(
                            "error while sending data to downstream #{} error: {}",
                            info.id, e
                        );
                        break;
                    }
                    Ok(_) => {}
                }
            }
        }
    }
    // clean up: removing the current UpstreamInfo from infos
    let mut writable_infos = infos.write().await;
    let pos = writable_infos
        .iter()
        .position(|i| i.id == info.id)
        .expect("should find socket info");
    writable_infos.remove(pos);
}

pub enum Mode {
    Encode,
    Decode,
}

impl Mode {
    pub fn build_begin_position(&self, prepend_size: usize) -> (usize, usize) {
        match self {
            Mode::Encode => (prepend_size, 0),
            Mode::Decode => (0, prepend_size),
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
                self.upstream_infos
                    .read()
                    .await
                    .iter()
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
                    self.upstream_infos.write().await.push(info.clone());
                    info
                }
            };

            match info
                .upstream_socket
                .send_to(&buf[send_bg_pos..recv_bg_pos + rev_size], &self.upstream)
                .await
            {
                Err(e) => {
                    warn!("error while sending data to upstream #{}: {}", info.id, e);
                }
                Ok(_) => {}
            }
        }
    }
}
