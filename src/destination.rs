
use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use clap::Parser;
use std::io::ErrorKind;

use crate::packet::PacketStruct;
use crate::packet::PacketType;

const PONG_PORT_INC: u16 = 1024;

#[derive(Parser)]
pub struct Args {
    pub port: u16,
    pub duration: u32,
    pub calc_rtt : bool,
    pub rx_mode: bool,
}

pub struct RecvRecord {
    pub packets: HashMap<u16, PacketStruct>, // Use a HashMap to store packets by their offset
    indicators: (bool, bool, bool, bool),
}

impl RecvRecord {
    fn new() -> Self{
        Self{
            packets: HashMap::<u16, PacketStruct>::new(),
            indicators: (false, false, false, false),
        }
    }
    fn record(&mut self, data: &[u8]){
        let packet = PacketStruct::from_buffer(data);
        match PacketStruct::get_packet_type(packet.indicators) {
            PacketType::SL  => {self.indicators.0 = true;},
            PacketType::DFL => {self.indicators.1 = true;},
            PacketType::DSL => {self.indicators.2 = true;},
            PacketType::DSF => {self.indicators.3 = true;},
            PacketType::DSS => {self.indicators.2 = true; self.indicators.3 = true;},
            _ => {}
        }
        self.packets.insert(packet.offset as u16, packet);
    }
    fn complete(&self) -> bool{
        if self.indicators.0 || (self.indicators.1 && self.indicators.2 && self.indicators.3) {
            let num_packets = self.packets.len();
            if num_packets == 0 {
                return false; // No packets, return false
            }
            for i in 0..num_packets {
                if !self.packets.contains_key(&(i as u16)) {
                    return false;
                }
            }
            return true;
        }
        return false;
    }
    #[allow(dead_code)]
    fn gather(&self) -> Vec<u8>{
        let mut data = Vec::new();
        let num_packets = self.packets.len();
        for i in 0..num_packets{
            let packet = self.packets.get(&(i as u16)).unwrap();
            data.extend_from_slice(&packet.payload[ ..packet.length as usize]);
        }
        return data;
    }
}

pub struct RecvData{
    pub recv_records: HashMap<u32, RecvRecord>,
    pub last_seq: u32,
    pub recevied: u32,
    pub data_len: u32,
    pub rx_start_time: f64,
    pub tx: Option<Sender<Vec<u8>>>
}

impl RecvData{
    pub fn new() -> Self{
        Self{
            recv_records: HashMap::new(),
            last_seq: 0,
            recevied: 0,
            data_len: 0,
            rx_start_time: 0.0,
            tx: None,
        }
    }
}

pub fn recv_thread(args: Args, recv_params: Arc<Mutex<RecvData>>, lock: Arc<Mutex<bool>>){
    let addr = format!("0.0.0.0:{}", args.port);    
    let socket = UdpSocket::bind(&addr).unwrap();
    socket.set_nonblocking(true).unwrap();

    let addr = format!("0.0.0.0:0");
    let pong_socket = UdpSocket::bind(&addr).unwrap();
    pong_socket.set_nonblocking(true).unwrap();

    let temp_socket = UdpSocket::bind("0.0.0.0:42639").unwrap();
    temp_socket.set_nonblocking(true).unwrap();

    println!("Waiting ...");
    let mut buffer = [0; 10240];
    loop {
        if let Ok((_len, _src_addr)) = socket.recv_from(&mut buffer) {
            *lock.lock().unwrap() = true;
            println!("Start");
            let mut data = recv_params.lock().unwrap();
            data.data_len += _len as u32;
            data.rx_start_time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64();

            match args.calc_rtt {
                true => {
                    let seq = u32::from_le_bytes(buffer[0..4].try_into().unwrap());
                    data.recv_records.entry(seq).or_insert_with(|| RecvRecord::new()).record(&buffer);               
                    match args.rx_mode {
                        true => {
                            if data.recv_records[&seq].complete() {
                                let recv_packet = data.recv_records[&seq].gather();
                                if let Some(tx) = &data.tx {
                                    tx.send(recv_packet).unwrap();
                                }
                                // let _ = temp_socket.send_to(&recv_packet, "10.25.11.100:12345");
                            }
                        },
                        false => {}
                    }
                    if data.recv_records[&seq].complete() { //TODO: deal with removing of uncompleted packets
                        data.recv_records.remove(&seq);
                        data.recevied += 1;
                    }
                },
                false => {},
            }

            break;
        }
        else {
            std::thread::sleep(std::time::Duration::from_nanos(100_000) );
        }
    }

    loop {
        let mut buffer = [0; 2048];
        if let Ok((_len, src_addr)) = socket.recv_from(&mut buffer) {
            let mut data = recv_params.lock().unwrap();
            data.data_len += _len as u32;

            match args.calc_rtt {
                true => {
                    let seq = u32::from_le_bytes(buffer[0..4].try_into().unwrap());
                    data.last_seq = seq;
                    data.recv_records.entry(seq).or_insert_with(|| RecvRecord::new()).record(&buffer);      
                    match args.rx_mode {
                        true => {
                            if data.recv_records[&seq].complete() {
                                let recv_packet = data.recv_records[&seq].gather();
                                if let Some(tx) = &data.tx {
                                    tx.send(recv_packet).unwrap();
                                }
                                // let _ = temp_socket.send_to(&recv_packet, "10.25.11.100:12345");
                            }
                        },
                        false => {}
                    }
                    if data.recv_records[&seq].complete() {
                        data.recv_records.remove(&seq);
                        data.recevied += 1;
                    }
                },
                false => {},
            }

            let indicator = u8::from_le_bytes(buffer[10..11].try_into().unwrap());
            match PacketStruct::get_packet_type(indicator) {
                PacketType::SL | PacketType::DFL | PacketType::DSL => {
                    let ping_addr = format!("{}:{}", src_addr.ip(), args.port + PONG_PORT_INC);
                    buffer[18..19].copy_from_slice(( indicator ).to_le_bytes().as_ref());
                    loop {
                        match pong_socket.send_to(&mut buffer[.._len], &ping_addr) {
                            Ok(_) => {break;},
                            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                                println!("Send operation would block, retrying later...");
                            }
                            Err(e) => {
                                eprintln!("Error sending data: {}", e);
                                break;
                            }
                        }
                    }
                },
                _ => {}
            }

        }
    }
}
