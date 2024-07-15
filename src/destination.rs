
use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use clap::Parser;
use log::trace;
use std::io::ErrorKind;

use crate::packet::{self, PacketStruct, PacketType};

use crate::socket::{*};

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
#[derive(Default)]
struct RecvOffsets {
    sl: Option<u16>,
    dfl: Option<u16>,
    dsf: Option<u16>,
    dsl: Option<u16>,
}

#[derive(Default)]
struct RecvComplete {
    sl_complete: bool,
    ch1_complete: bool,
    ch2_complete: bool,
}

type IsACK = (bool, bool);

pub struct RecvRecord {
    pub packets: HashMap<u16, PacketStruct>, // Use a HashMap to store packets by their offset
    offsets: RecvOffsets,
    is_complete: RecvComplete,
    is_ack: IsACK,
}

impl RecvRecord {
    fn new() -> Self{
        Self{
            packets: HashMap::<u16, PacketStruct>::new(),
            offsets: RecvOffsets::default(),
            is_complete: RecvComplete::default(),
            is_ack : (false, false),
        }
    }
    fn record(&mut self, data: &[u8]) {
        let packet = packet::from_buffer(data);
        let offset = Some(packet.offset);

        match packet::get_packet_type(packet.indicators) {
            PacketType::SL  => self.offsets.sl = offset,
            PacketType::DFL => self.offsets.dfl = offset,
            PacketType::DSF => self.offsets.dsf = offset,
            PacketType::DSL => self.offsets.dsl = offset,
            PacketType::DSS => {
                self.offsets.dsf = offset;
                self.offsets.dsl = offset;
            }
            _ => {}
        }

        self.packets.insert(packet.offset as u16, packet);
        self.is_complete = self.determine_complete();
    }

    fn is_complete(&self) -> bool {
        self.is_complete.sl_complete || (self.is_complete.ch1_complete && self.is_complete.ch2_complete)
    }

    fn is_fst_ack(&self) -> bool {
        !self.is_ack.0 && (self.is_complete.sl_complete || self.is_complete.ch1_complete)
    }

    fn is_scd_ack(&self) -> bool {
        !self.is_ack.1 && self.is_complete.ch2_complete
    }

    fn determine_complete(&self) -> RecvComplete {
        fn is_range_complete(packets: &HashMap<u16, PacketStruct>, mut range: std::ops::RangeInclusive<u16>) -> bool {
            range.all(|i| packets.contains_key(&i))
        }
    
        if let Some(sl) = self.offsets.sl {
            if is_range_complete(&self.packets, 0..=sl) {
                return RecvComplete { sl_complete: true, ch1_complete: false, ch2_complete: false };
            }
        }
    
        let ch1_complete = self.offsets.dfl.map_or(false, |dfl| is_range_complete(&self.packets, 0..=dfl));
        let ch2_complete = match (self.offsets.dsf, self.offsets.dsl) {
            (Some(dsf), Some(dsl)) => is_range_complete(&self.packets, dsl..=dsf),
            _ => false,
        };

        RecvComplete { sl_complete: false, ch1_complete, ch2_complete }
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

pub fn recv_thread(args: Args, recv_params: Arc<Mutex<RecvData>>, lock: Arc<Mutex<bool>>){
    let addr = format!("0.0.0.0:{}", args.port);    
    let socket = UdpSocket::bind(&addr).unwrap();
    socket.set_nonblocking(true).unwrap();

    let addr = format!("0.0.0.0:0");
    // let pong_socket = UdpSocket::bind(&addr).unwrap();
    let pong_socket = create_udp_socket(192, addr.clone());
    if let Some(pong_socket) = pong_socket {
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
    
                match args.calc_rtt {
                    true => {
                        let seq = u32::from_le_bytes(buffer[0..4].try_into().unwrap());
                        data.recv_records.entry(seq).or_insert_with(|| RecvRecord::new()).record(&buffer);     
                        let _record = &data.recv_records[&seq];          
                        match args.rx_mode {
                            true => {
                                if _record.is_complete()  {
                                    let _ = _record.gather();
                                }
                            },
                            false => {}
                        }
                        if _record.is_complete()  { //TODO: deal with removing of uncompleted packets
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
                        let mut _record = &mut data.recv_records.get_mut(&seq).unwrap();
                        match args.rx_mode {
                            true => {
                                if  _record.is_complete() {
                                    let _ = _record.gather();
                                }
                            },
                            false => {}
                        }
                        
                        if  _record.is_fst_ack() || _record.is_scd_ack() {
                            let packet_type = if src_addr.ip().to_string() == args.src_ipaddrs[0]{
                                trace!("ACKFirst: Time {} -> seq: {}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64(), seq);
                                _record.is_ack.0 = true;
                                PacketType::DFL
                            } else {
                                trace!("ACKSecond: Time {} -> seq: {}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64(), seq);
                                _record.is_ack.1 = true;
                                PacketType::DSL
                            };
                            buffer[18..19].copy_from_slice(packet::to_indicator(packet_type).to_le_bytes().as_ref());
                            let ping_addr = format!("{}:{}", src_addr.ip(), args.port + PONG_PORT_INC);
                            send_ack(&pong_socket, &mut buffer[.._len], &ping_addr);
                        }
    
                        if  _record.is_complete() {
                            data.recv_records.remove(&seq);
                            data.recevied += 1;
                        }
                    },
                    false => {},
                }
            }
        }
    }
    else {
        eprintln!("Error creating pong socket");
    }
}