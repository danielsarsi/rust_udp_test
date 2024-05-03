use log::{debug, info, warn};
use std::{
    io,
    sync::{
        atomic::{AtomicIsize, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{net::UdpSocket, task, time};
use uuid::{Uuid, Version};

#[tokio::main]
async fn main() -> io::Result<()> {
    env_logger::init();

    let packets_received = Arc::new(AtomicIsize::new(0));
    let packets_sent = Arc::new(AtomicIsize::new(0));
    let bytes_received = Arc::new(AtomicUsize::new(0));
    let bytes_sent = Arc::new(AtomicUsize::new(0));

    let packets_received_clone = packets_received.clone();
    let packets_sent_clone = packets_sent.clone();
    let bytes_received_clone = bytes_received.clone();
    let bytes_sent_clone = bytes_sent.clone();

    let addr = std::env::var("BIND_ADDR").unwrap_or(String::from("0.0.0.0:2054"));
    let sock = UdpSocket::bind(&addr).await?;

    info!("listening on: {}", addr);

    task::spawn(async move {
        loop {
            let packets_received_last = packets_received_clone.swap(0, Ordering::Relaxed);
            let packets_sent_last = packets_sent_clone.swap(0, Ordering::Relaxed);
            let bytes_received_last = bytes_received_clone.swap(0, Ordering::Relaxed);
            let bytes_sent_last = bytes_sent_clone.swap(0, Ordering::Relaxed);

            info!(
                "packets received: {}, packets sent: {}, bytes received: {}, bytes sent: {}",
                packets_received_last, packets_sent_last, bytes_received_last, bytes_sent_last
            );

            time::sleep(Duration::from_secs(10)).await;
        }
    });

    let mut buf = [0; 512];

    loop {
        let (len, addr) = sock.recv_from(&mut buf).await?;

        debug!("received {} bytes from {}", len, addr);

        packets_received.fetch_add(1, Ordering::Relaxed);
        bytes_received.fetch_add(len, Ordering::Relaxed);

        // split buffer by new line
        let lines: Vec<_> = buf.split(|&x| x == b'\n').collect();

        // first line should be "RTT"
        if lines[0] != b"RTT" {
            warn!(
                "invalid packet received with first line: {:?} from {}",
                lines[0], addr
            );
            continue;
        }

        // second line should be an UUID
        let uuid = Uuid::try_parse_ascii(lines[1]);

        if uuid.is_err() || !uuid.is_ok_and(|u| u.get_version() == Some(Version::Random)) {
            warn!(
                "invalid packet received with second line: {:?} from {}",
                lines[1], addr
            );
            continue;
        }

        if lines[2].len() < 6 {
            warn!(
                "invalid packet received with third line: {:?} from {}",
                lines[2], addr
            );
            continue;
        }

        // last byte of third line should be a number between 1 and 9
        let last_byte = lines[2].last();
        if last_byte < Some(&b'0') || last_byte > Some(&b'9') {
            warn!(
                "invalid packet received with third line: {:?} from {}",
                lines[2], addr
            );
            continue;
        }

        let len = sock.send_to(&buf[..len], addr).await?;
        debug!("sent {} bytes to {}", len, addr);

        packets_sent.fetch_add(1, Ordering::Relaxed);
        bytes_sent.fetch_add(len, Ordering::Relaxed);
    }
}
