use clap::Parser;
use std::{
    collections::HashMap,
    io,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::net::UdpSocket;
use uuid::Uuid;

#[derive(Debug)]
struct UDPTestMessage {
    uuid: Uuid,
    sequence: u64,
    size: u64,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Address to send packets to
    #[arg(short, long, value_parser = clap::value_parser!(String))]
    address: String,

    /// Local port to bind to
    #[arg(short, long, default_value_t = "127.0.0.1:2054", value_parser = clap::value_parser!(String))]
    local_address: String,

    /// Number of packets to send
    #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u64))]
    packets_number: u64,

    /// Packets size, in bytes
    #[arg(long, default_value_t = 64, value_parser = clap::value_parser!(u64).range(64..513))]
    packets_size: u64,

    /// Interval between sending packets, in milliseconds
    #[arg(long, default_value_t = 1000, value_parser = clap::value_parser!(u64).range(0..))]
    interval: u64,

    /// Time to wait for response, in milliseconds
    #[arg(long, default_value_t = 1000, value_parser = clap::value_parser!(u64).range(0..))]
    timeout: u64,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let args = Args::parse();

    let local_address = args.local_address
        .parse::<SocketAddr>()
        .unwrap();

    println!("Local address: {}", local_address);

    let sock = UdpSocket::bind(
        format!("0.0.0.0:{}", args.local_port)
            .parse::<SocketAddr>()
            .unwrap(),
    )
    .await?;

    let r = Arc::new(sock);
    let s = r.clone();

    let transmitted = Arc::new(Mutex::new(HashMap::<u64, (u64, Instant)>::new()));
    let received = Arc::new(Mutex::new(HashMap::<u64, (u64, Instant)>::new()));

    let transmitted_clone = transmitted.clone();
    let received_clone = received.clone();

    tokio::spawn(async move {
        let mut buf = [0; 512];

        while let Ok((len, addr)) = r.recv_from(&mut buf).await {
            let now = Instant::now();
            let bytes = buf[..len].to_vec();

            if let Some(message) = parse_udp_test_message(&bytes) {
                let transmitted = transmitted_clone
                    .lock()
                    .unwrap()
                    .get(&message.sequence)
                    .unwrap()
                    .1;

                if transmitted.elapsed().as_millis() > args.timeout.try_into().unwrap() {
                    println!("Timeout: {:?}", message);
                    continue;
                }

                received_clone
                    .lock()
                    .unwrap()
                    .insert(message.sequence, (message.size, now));

                println!("Received: {:?}", message);
            }

            println!("received {} bytes from {}", len, addr);
        }
    });

    let address = args.address.parse::<SocketAddr>().unwrap();
    let started_at = Instant::now();

    let uuid = Uuid::new_v4();

    for i in 1..args.packets_number + 1 {
        let message = UDPTestMessage {
            uuid,
            sequence: i,
            size: args.packets_size,
        };

        let packet = create_udp_test_message(&message).unwrap();
        let len = s.send_to(&packet, address).await.unwrap();

        let now = Instant::now();
        transmitted
            .lock()
            .unwrap()
            .insert(message.sequence, (len.try_into().unwrap(), now));

        println!("Sent: {:?}", message);

        if i == args.packets_number {
            break;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(args.interval)).await;
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(args.timeout)).await;

    let ended_at = Instant::now();

    let duration = ended_at - started_at;

    let packets_transmitted = transmitted.lock().unwrap().len();
    let packets_received = received.lock().unwrap().len();
    let packets_lost = packets_transmitted - packets_received;

    let mut bytes_total = 0;

    let mut packets_rtt = Vec::<u64>::new();

    for (sequence, (transmitted_bytes, sent_at)) in transmitted.lock().unwrap().iter() {
        bytes_total += transmitted_bytes;

        if let Some((received_bytes, received_at)) = received.lock().unwrap().get(sequence) {
            let rtt = *received_at - *sent_at;
            packets_rtt.push(rtt.as_millis().try_into().unwrap());

            bytes_total += received_bytes;
        }
    }

    let rtt_sum: u64 = packets_rtt.iter().sum();
    let rtt_len = packets_rtt.len();
    let rtt_avg: f64 = rtt_sum as f64 / rtt_len as f64;

    println!(
        "Packets transmitted: {}, packets received: {}, packets lost: {}, duration: {:?}, bytes total: {}, RTT avg: {} ms",
        packets_transmitted, packets_received, packets_lost, duration, bytes_total, rtt_avg
    );

    Ok(())
}

fn create_udp_test_message(message: &UDPTestMessage) -> Option<Vec<u8>> {
    let padded_sequence = format!("{:0>6}", message.sequence);
    let packet = format!("RTT\n{}\n{}\n", message.uuid, padded_sequence);

    let mut packet = packet.as_bytes().to_vec();
    packet.resize(message.size.try_into().unwrap(), 0);

    Some(packet)
}

fn parse_udp_test_message(data: &[u8]) -> Option<UDPTestMessage> {
    let lines: Vec<_> = data.split(|&x| x == b'\n').collect();

    if lines[0] != b"RTT" {
        return None;
    }

    let uuid = Uuid::try_parse_ascii(lines[1]).ok()?;
    let sequence = u64::from_str_radix(std::str::from_utf8(lines[2]).ok()?, 10).ok()?;

    Some(UDPTestMessage {
        uuid,
        sequence,
        size: data.len().try_into().unwrap(),
    })
}
