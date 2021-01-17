
use clap::*;
use log::{error, info, LevelFilter, warn, debug};
use async_std::task;
use async_std::sync::Arc;
use std::thread;
use udp_prepend::logger;
use async_std::net::{UdpSocket, SocketAddr, ToSocketAddrs};
const BUFFER_SIZE: usize = 2048;

struct UpstreamInfo {
    downstream_addr: SocketAddr,
    upstream_socket: UdpSocket,
    last_active_ts: u64,
}

impl UpstreamInfo {
    async fn recv(&self, downstream_socket: &UdpSocket, mode: Mode, prepend_size: usize) {
        let (recv_bg_pos, send_bg_pos) = mode.build_begin_position(prepend_size);
        let mut buf = [0u8; BUFFER_SIZE];
        loop {
            let (rev_size, addr) = self.upstream_socket.recv_from(&mut buf[recv_bg_pos..]).await.expect("should recv success");
            for i in 0..recv_bg_pos {
                buf[i] = buf[recv_bg_pos + rev_size - i - 1]
            }
            downstream_socket.send_to(&buf[send_bg_pos..recv_bg_pos + rev_size], &self.downstream_addr).await;
        }
    }
}

enum Mode {
    Encode,
    Decode,
}

impl Mode {
    fn build_begin_position(&self, prepend_size: usize) -> (usize, usize) {
        match self {
            Mode::Decode => (prepend_size, 0),
            Mode::Encode => (0, prepend_size),
        }
    }
    fn reverse(&self) -> Mode {
        match self {
            Mode::Decode => Mode::Encode,
            Mode::Encode => Mode::Decode,
        }
    }
}

struct UdpPrepend {
    prepend_size: usize,
    mode: Mode,
    bind: SocketAddr,
    upstream: SocketAddr,
    downstream_socket: UdpSocket,
    upstream_infos: Vec<UpstreamInfo>,
}

impl UdpPrepend {

    async fn new() {
        let downstream_socket = UdpSocket::bind("TODO").await.expect("failed to bind local socket");
    }

    async fn main(&'static mut self) {
        let mut buf = [0u8; BUFFER_SIZE];
        let (recv_bg_pos, send_bg_pos) = self.mode.build_begin_position(self.prepend_size);

        loop {
            let (rev_size, downstream_addr) = self.downstream_socket.recv_from(&mut buf[recv_bg_pos..]).await.expect("should get date success");
            for i in 0..recv_bg_pos {
                buf[i] = buf[recv_bg_pos + rev_size - i - 1]
            }
            let info = match self.upstream_infos.iter().find(|&info| info.downstream_addr.eq(&downstream_addr)) {
                Some(pair) => pair,
                None => {
                    let upstream_socket = UdpSocket::bind("0.0.0.0:0").await.expect("failed to bind addr for upstream conn");
                    let info: UpstreamInfo = UpstreamInfo {
                        downstream_addr,
                        upstream_socket,
                        last_active_ts: 0,
                    };
                    let b_info: &'static UpstreamInfo = &info;
                    self.upstream_infos.push(info);
                    // let info = self.upstream_infos.last().unwrap();
                    task::spawn(b_info.recv(&self.downstream_socket, self.mode.reverse(), self.prepend_size));
                    // self.upstream_infos.last().unwrap()
                    b_info
                }
            };
            info.upstream_socket.send_to(&buf[send_bg_pos..recv_bg_pos + rev_size], &self.upstream).await;
        }
    }
}

fn main() {
    log::set_logger(&logger::LOGGER).unwrap();
    log::set_max_level(LevelFilter::Info);

    let matches = App::new(crate_name!())
        .version(crate_version!())
        .about(crate_description!())
        .arg(Arg::with_name("bind")
            .short("b")
            .long("bind")
            .value_name("BIND")
            .help("the local address to bind")
            .required(true))
        .arg(Arg::with_name("upstream")
            .short("r")
            .long("upstream")
            .value_name("UPSTREAM")
            .help("the upstream address to connect")
            .required(true))
        .arg(Arg::with_name("size")
            .short("s")
            .long("size")
            .value_name("SIZE")
            .help("the prepend size in bytes, default 4 bytes")
            .required(false)
            .default_value("4"))
        .arg(Arg::with_name("mode")
            .short("m")
            .long("mode")
            .value_name("MODE")
            .help("working mode, either 'encode' or 'decode'")
            .required(true))
        .get_matches();

    let bind: SocketAddr = value_t!(matches.value_of("bind"), SocketAddr).expect("bind address is required");
    let upstream: SocketAddr = value_t!(matches.value_of("upstream"), SocketAddr).expect("upstream address is required");
    let size: usize = value_t!(matches.value_of("size"), usize).expect("size is required");
    let mode: String = value_t!(matches.value_of("mode"), String).expect("size is required");

    if mode == "encode" {

    } else if mode == "decode" {

    } else {
        return
    }




    return
}

