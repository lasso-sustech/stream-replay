mod packet;
use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use clap::Parser;
use std::io::ErrorKind;

use crate::packet::PacketStruct;

const PONG_PORT_INC: u16 = 1024;

#[derive(Parser)]
struct Args {
    port: u16,
    duration: u32,
    calc_rtt : bool,
    rx_mode: bool,
}

struct RecvRecord{
    packets: HashMap<u16, PacketStruct>, // Use a HashMap to store packets by their offset
    indicators: (bool, bool),
}

impl RecvRecord{
    fn new() -> Self{
        Self{
            packets: HashMap::<u16, PacketStruct>::new(),
            indicators: (false, false),
        }
    }
    fn record(&mut self, data: &[u8]){
        let packet = PacketStruct::from_buffer(data);
        if packet.indicators == 2 {
            self.indicators.0 = true;
        }
        else if packet.indicators == 3 {
            self.indicators.1 = true;
        }
        self.packets.insert(packet.offset as u16, packet);
    }
    fn complete(&self) -> bool{
        if self.indicators.0 && self.indicators.1 {
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
    fn gather(&self) -> Vec<u8>{
        let mut data = Vec::new();
        // print all keys
        let num_packets = self.packets.len();
        for i in 0..num_packets{
            let packet = self.packets.get(&(i as u16)).unwrap();
            data.extend_from_slice(&packet.payload[..]);
        }
        return data;
    }
}

struct RecvData{
    recv_records: HashMap<u32, RecvRecord>,
    data_len: u32,
    rx_start_time: f64,
}

impl RecvData{
    fn new() -> Self{
        Self{
            recv_records: HashMap::new(),
            data_len: 0,
            rx_start_time: 0.0,
        }
    }
}

fn main() {
    let args = Args::parse();
    let recv_data = Arc::new(Mutex::new(RecvData::new()));
    let recv_data_thread = Arc::clone(&recv_data);

    // Extract duration from args
    let duration = args.duration.clone();
    
    let lock = Arc::new(Mutex::new(false));
    let lock_clone = Arc::clone(&lock);
    
    std::thread::spawn(move || {
        recv_thread(args, recv_data_thread, lock_clone);
    });

    while !*lock.lock().unwrap() {
        std::thread::sleep(std::time::Duration::from_nanos(100_000) );
    }

    // Sleep for the duration
    std::thread::sleep(std::time::Duration::from_secs(duration as u64));

    let data_len = recv_data.lock().unwrap().data_len;
    let rx_duration = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64() - recv_data.lock().unwrap().rx_start_time;

    println!("Received Bytes: {:.3} MB", data_len as f64/ 1024.0 / 1024.0);
    println!("Average Throughput: {:.3} Mbps", data_len as f64 / rx_duration / 1e6 * 8.0);
}

fn recv_thread(args: Args, recv_params: Arc<Mutex<RecvData>>, lock: Arc<Mutex<bool>>){
    let rx_mode = args.rx_mode;
    let addr = format!("0.0.0.0:{}", args.port);    
    let socket = UdpSocket::bind(&addr).unwrap();
    socket.set_nonblocking(true).unwrap();

    let addr = format!("0.0.0.0:0");
    let pong_socket = UdpSocket::bind(&addr).unwrap();
    pong_socket.set_nonblocking(true).unwrap();
    println!("Waiting ...");
    let mut buffer = [0; 10240];
    loop {
        if let Ok((_len, _src_addr)) = socket.recv_from(&mut buffer) {
            *lock.lock().unwrap() = true;
            println!("Start");
            let mut data = recv_params.lock().unwrap();
            data.data_len += _len as u32;
            data.rx_start_time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64();

            if !args.calc_rtt {
                break;
            }

            if rx_mode {
                let seq = u32::from_le_bytes(buffer[0..4].try_into().unwrap());
                data.recv_records.entry(seq).or_insert_with(|| RecvRecord::new()).record(&buffer);                
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

            if !args.calc_rtt {
                continue;
            }
            
            if rx_mode {
                let seq = u32::from_le_bytes(buffer[0..4].try_into().unwrap());
                data.recv_records.entry(seq).or_insert_with(|| RecvRecord::new()).record(&buffer);               
                if data.recv_records[&seq].complete() {
                    data.recv_records[&seq].gather();
                    data.recv_records.remove(&seq);
                }
            }

            let indicator = u8::from_le_bytes(buffer[10..11].try_into().unwrap());
            if (indicator == 2) || (indicator == 3) {
                let modified_addr = format!("{}:{}", src_addr.ip(), args.port + PONG_PORT_INC);
                buffer[18..19].copy_from_slice((indicator ).to_le_bytes().as_ref());
                loop {
                    match pong_socket.send_to(&mut buffer[.._len], &modified_addr) {
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
            }

        }
    }
}