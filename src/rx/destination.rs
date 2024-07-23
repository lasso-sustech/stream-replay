use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use clap::Parser;
use log::trace;
use std::io::ErrorKind;

use crate::record::{RecvData, RecvRecord};
use stream_replay::core::packet::{self, PacketType};
use stream_replay::core::socket::{*};

const PONG_PORT_INC: u16 = 1024;

#[derive(Parser)]
pub struct Args {
    pub port: u16,
    pub duration: u32,
    pub calc_rtt : bool,
    pub rx_mode: bool,
    #[clap(long)]
    pub src_ipaddrs: Vec<String>,
}


fn send_ack(pong_socket: &UdpSocket, buffer: &[u8], ping_addr: &str) {
    loop {
        match pong_socket.send_to(&buffer, ping_addr) {
            Ok(_) => break,
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                println!("Send operation would block, retrying later...");
            }
            Err(e) => {
                eprintln!("Error sending data: {}", e);
                break;
            }
        }
    }
}

fn handle_rtt(args: &Args, buffer: &mut [u8], data: &mut RecvData, pong_socket: &UdpSocket, src_addr: &std::net::SocketAddr) -> Option<Vec<u8>> {
    let seq = u32::from_le_bytes(buffer[0..4].try_into().unwrap());
    data.last_seq = if seq > data.last_seq { seq } else { data.last_seq };
    
    data.recv_records.entry(seq).or_insert_with(RecvRecord::new).record(buffer);
    let _record = data.recv_records.get_mut(&seq).unwrap();
    let mut res = None;

    if _record.is_fst_ack() || _record.is_scd_ack() {
        let packet_type = if src_addr.ip().to_string() == args.src_ipaddrs[0] {
            trace!("ACKFirst: Time {} -> seq: {}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64(), seq);
            _record.is_ack.0 = true;
            if _record.is_complete() {
                PacketType::SLFL
            } else {
                PacketType::DFL
            }
        } else {
            trace!("ACKSecond: Time {} -> seq: {}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64(), seq);
            _record.is_ack.1 = true;
            if _record.is_complete() {
                PacketType::SLSL
            } else {
                PacketType::DSL
            }
        };
        buffer[18..19].copy_from_slice(packet::to_indicator(packet_type).to_le_bytes().as_ref());
        let ping_addr = format!("{}:{}", src_addr.ip(), args.port + PONG_PORT_INC);
        send_ack(pong_socket, buffer, &ping_addr);
    }

    if _record.is_complete() {
        if args.rx_mode {
            res = Some(_record.gather());
        }
        data.recv_records.remove(&seq);
        data.recevied += 1;
    }
    
    res
}

pub fn recv_thread(args: Args, recv_params: Arc<Mutex<RecvData>>, lock: Arc<Mutex<bool>>){
    let addr = format!("0.0.0.0:{}", args.port);    
    let socket = UdpSocket::bind(&addr).unwrap();
    socket.set_nonblocking(true).unwrap();

    let addr = format!("0.0.0.0");
    // let pong_socket = UdpSocket::bind(&addr).unwrap();
    let pong_socket = create_udp_socket(192, addr.clone());
    if let Some(pong_socket) = pong_socket {
        pong_socket.set_nonblocking(true).unwrap();
        println!("Waiting ...");
        let mut buffer = [0; 2048];
        let mut started = false;
        loop {
            if let Ok((_len, src_addr)) = socket.recv_from(&mut buffer) {
                let mut data = recv_params.lock().unwrap();
                data.data_len += _len as u32;
                if !started {
                    *lock.lock().unwrap() = true;
                    println!("Start");
                    data.rx_start_time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64();
                    started = true;
                }

                if args.calc_rtt {
                    handle_rtt(&args, &mut buffer, &mut data, &pong_socket, &src_addr);
                }

            } else if !started {
                std::thread::sleep(std::time::Duration::from_nanos(100_000));
            }
        }
    }
    else {
        eprintln!("Error creating pong socket");
    }
}