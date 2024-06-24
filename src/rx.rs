
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use clap::Parser;
use std::io::ErrorKind;



const PONG_PORT_INC: u16 = 1024;

#[derive(Parser)]
pub struct Args {
    pub port: u16,
    pub duration: u32,
    pub calc_rtt : bool
}

struct RecvRecord{
    offset_num : u16,
}

impl RecvRecord{
    fn new() -> Self{
        Self{
            offset_num: 0,
        }
    }
}
pub struct RecvData{
    recv_records: Vec<RecvRecord>,
    pub data_len: u32,
    pub rx_start_time: f64,
}



impl RecvData{
    pub fn new() -> Self{
        Self{
            recv_records: Vec::new(),
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
    println!("Received Bytes: {:.3} MB", data_len as f64/ 1024.0 / 1024.0);
    let rx_duration = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64() - recv_data.lock().unwrap().rx_start_time;
    println!("Average Throughput: {:.3} Mbps", data_len as f64 / rx_duration / 1e6 * 8.0);
}

pub fn recv_thread(args: Args, recv_params: Arc<Mutex<RecvData>>, lock: Arc<Mutex<bool>>){
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
            let seq = u32::from_le_bytes(buffer[0..4].try_into().unwrap());
            let _offset = u16::from_le_bytes(buffer[4..6].try_into().unwrap());

            while data.recv_records.len() <= seq as usize {
                data.recv_records.push(RecvRecord::new());
            }
            data.recv_records[seq as usize].offset_num += 1;
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

            let seq = u32::from_le_bytes(buffer[0..4].try_into().unwrap());
            let _offset = u16::from_le_bytes(buffer[4..6].try_into().unwrap());
            let indicator = u8::from_le_bytes(buffer[10..11].try_into().unwrap());
            
            while data.recv_records.len() <= seq as usize {
                data.recv_records.push(RecvRecord::new());
            }

            data.recv_records[seq as usize].offset_num += 1;

            // if data.recv_records[seq as usize].offset_num == num {
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