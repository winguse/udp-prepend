use crate::udp_prepend::*;
use async_std::task;
use std::net::UdpSocket;
use std::time::Duration;

#[test]
fn work_correctly() {
    let encode_join = task::spawn(async {
        let encode = UdpPrepend::new(
            16,
            Mode::Encode,
            "127.0.0.1:54321".parse().unwrap(),
            "127.0.0.1:54322".parse().unwrap(),
            Duration::from_secs(60),
        )
        .await;
        encode.start_forward().await
    });

    let decode_join = task::spawn(async {
        let decode = UdpPrepend::new(
            16,
            Mode::Decode,
            "127.0.0.1:54322".parse().unwrap(),
            "127.0.0.1:54323".parse().unwrap(),
            Duration::from_secs(60),
        )
        .await;
        decode.start_forward().await
    });

    let receiver = UdpSocket::bind("127.0.0.1:54323").unwrap();

    let sender = UdpSocket::bind("127.0.0.1:54320").unwrap();
    sender
        .send_to("hello".as_bytes(), "127.0.0.1:54321")
        .unwrap();

    let mut receive_buf = [0u8; 1024];

    let (size, addr) = receiver.recv_from(&mut receive_buf).unwrap();
    assert_eq!("hello", String::from_utf8_lossy(&receive_buf[..size]));
    receiver
        .send_to(
            "hello world response, a very long string for testing".as_bytes(),
            addr,
        )
        .unwrap();

    let size = sender.recv(&mut receive_buf).unwrap();
    assert_eq!(
        "hello world response, a very long string for testing",
        String::from_utf8_lossy(&receive_buf[..size])
    );

    task::block_on(encode_join.cancel());
    task::block_on(decode_join.cancel());
}
