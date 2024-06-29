use std::fs::File;
use std::io::prelude::*;
use std::collections::HashMap;
use std::net::UdpSocket;
use std::thread::{self, JoinHandle};
use std::sync::{Arc, Mutex, mpsc};
use std::time::SystemTime;

use crate::packet::PacketStruct;
use crate::statistic::rtt_records::RttRecords;

type SeqRecords = HashMap<u32,f64>;
type GuardedSeqRecords = Arc<Mutex<SeqRecords>>;

type GuardedRttRecords = Arc<Mutex<RttRecords>>;
pub type RttSender = mpsc::Sender<u32>;
type RttReceiver = mpsc::Receiver<u32>;
static PONG_PORT_INC:u16 = 1024;

pub struct RttRecorder {
    record_handle: Option<JoinHandle<()>>,
    recv_handle: Option<JoinHandle<()>>,
    name: String,
    port: u16,
    pub rtt_records: GuardedRttRecords,
    mul_link_num: usize,
}

struct LinkRttCount{
    link_rcev_count: HashMap<u32, usize>,
    max_link_num: usize,
}

fn record_thread(rx: RttReceiver, records: GuardedSeqRecords) {
    while let Ok(seq) = rx.recv() {
        let time_now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64();
        let mut _records = records.lock().unwrap();
        _records.insert(seq, time_now);
    }
}

fn pong_recv_thread(name: String, port: u16, seq_records: GuardedSeqRecords, rtt_records: GuardedRttRecords, tx_ipaddr:String, mut recv_counter: LinkRttCount) {
    let mut buf = [0; 2048];
    let sock = UdpSocket::bind( format!("{}:{}",tx_ipaddr, port)).unwrap();
    let mut logger = File::create( format!("logs/rtt-{}.txt", name) ).unwrap();

    while let Ok(_) = sock.recv_from(&mut buf) {
        let seq = u32::from_le_bytes( buf[..4].try_into().unwrap() );
        let indicator = u8::from_le_bytes( buf[18..19].try_into().unwrap() );
        let time_now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64();
        if let Some(last_time) = {
            let mut _records = seq_records.lock().unwrap();
            let _link_rtt_count = recv_counter.link_rcev_count.entry(seq).or_insert(0);
            *_link_rtt_count += 1;
            if *_link_rtt_count == recv_counter.max_link_num  {
                recv_counter.link_rcev_count.remove(&seq);
                _records.remove(&seq)
            }
            else {
                _records.get(&seq).cloned()
            }
        } {
            let rtt = time_now - last_time;
            rtt_records.lock().unwrap().update(seq as usize, PacketStruct::channel_info(indicator) as usize, rtt);
            let message = format!("{} {:.6} {:.6} \n", seq, rtt, PacketStruct::channel_info(indicator));
            logger.write_all( message.as_bytes() ).unwrap();
        };
    }
}

impl RttRecorder {
    pub fn new(name:&String, port:u16, mul_link_num: usize) -> Self {
        let name = name.clone();
        let port = port + PONG_PORT_INC; //pong recv port
        let record_handle = None;
        let recv_handle = None;
        let rtt_records = Arc::new(Mutex::new(RttRecords::new(1000, mul_link_num)));
        RttRecorder{ name, port, record_handle, recv_handle, rtt_records, mul_link_num }
    }

    pub fn start(&mut self,tx_ipaddr:String) -> RttSender {
        let (tx, rx) = mpsc::channel::<u32>();
        let (name, port) = (self.name.clone(), self.port);
        let seq_records1: GuardedSeqRecords = Arc::new(Mutex::new(HashMap::new()));
        let seq_records2 = seq_records1.clone();
        let rtt_records  = Arc::clone(&self.rtt_records);
        
        let recv_counter = LinkRttCount{ link_rcev_count: HashMap::new(), max_link_num: self.mul_link_num };

        self.record_handle = Some(
            thread::spawn(move || { record_thread(rx, seq_records1); })
        );
        self.recv_handle = Some(
            thread::spawn(move || { pong_recv_thread(name, port, seq_records2, rtt_records, tx_ipaddr, recv_counter ); } )
        );

        tx
    }

    
}
