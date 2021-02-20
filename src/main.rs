use std::time::Duration;

use async_std::net::SocketAddr;
use async_std::task;
use clap::*;
use log::LevelFilter;

use udp_prepend::logger;
use udp_prepend::udp_prepend::{Mode, UdpPrepend};

fn main() {
    log::set_logger(&logger::LOGGER).unwrap();
    log::set_max_level(LevelFilter::Info);

    let matches = App::new(crate_name!())
        .version(crate_version!())
        .about(crate_description!())
        .arg(
            Arg::with_name("bind")
                .short("b")
                .long("bind")
                .value_name("BIND")
                .help("the local address to bind")
                .required(true),
        )
        .arg(
            Arg::with_name("upstream")
                .short("r")
                .long("upstream")
                .value_name("UPSTREAM")
                .help("the upstream address to connect")
                .required(true),
        )
        .arg(
            Arg::with_name("size")
                .short("s")
                .long("size")
                .value_name("SIZE")
                .help("the prepend size in bytes, default 4 bytes")
                .required(false)
                .default_value("4"),
        )
        .arg(
            Arg::with_name("mode")
                .short("m")
                .long("mode")
                .value_name("MODE")
                .help("working mode, either 'encode' or 'decode'")
                .required(true),
        )
        .arg(
            Arg::with_name("timeout")
                .short("t")
                .long("timeout")
                .value_name("TIMEOUT")
                .help("upstream receive timeout in seconds")
                .required(true),
        )
        .get_matches();

    let bind: SocketAddr =
        value_t!(matches.value_of("bind"), SocketAddr).expect("bind address is required");
    let upstream: SocketAddr =
        value_t!(matches.value_of("upstream"), SocketAddr).expect("upstream address is required");
    let size: usize = value_t!(matches.value_of("size"), usize).expect("size is required");
    let mode: Mode = value_t!(matches.value_of("mode"), Mode).expect("size is required");
    let upstream_timeout: Duration = Duration::new(
        value_t!(matches.value_of("timeout"), u64).expect("timeout is required"),
        0,
    );

    task::block_on(async {
        let prepend = UdpPrepend::new(size, mode, bind, upstream, upstream_timeout).await;
        prepend.start_forward().await;
    })
}
