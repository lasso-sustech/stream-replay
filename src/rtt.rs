use std::fs::File;
use std::io::prelude::*;
use std::collections::HashMap;
use std::net::UdpSocket;
use std::thread::{self, JoinHandle};
use std::sync::{Arc, Mutex, mpsc};
use std::time::SystemTime;

type RttRecords = HashMap<u32,f64>;
type GuardRttRecords = Arc<Mutex<RttRecords>>;

pub struct RttRecorder {
    record_handle: Option<JoinHandle<()>>,
    recv_handle: Option<JoinHandle<()>>,
    name: String,
    port: u16
}

fn record_thread(rx: mpsc::Receiver<u32>, records: GuardRttRecords) {
    while let Ok(seq) = rx.recv() {
        let time_now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64();
        let mut _records = records.lock().unwrap();
        // println!("seq: {}", seq);
        _records.insert(seq, time_now);
    }
}

fn pong_recv_thread(name: String, port: u16, records: GuardRttRecords) {
    let mut buf = [0; 128];
    let sock = UdpSocket::bind( format!("0.0.0.0:{}", port)).unwrap();
    let mut logger = File::create( format!("data/rtt-{}.txt", name) ).unwrap();

    while let Ok(_) = sock.recv_from(&mut buf) {
        let msg: [u8;4] = buf[..4].try_into().unwrap();
        let seq = u32::from_le_bytes( msg );
        let msg: [u8;8] = buf[10..18].try_into().unwrap();
        let duration = f64::from_le_bytes( msg )

        let time_now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64();
        let last_time = {
            let mut _records = records.lock().unwrap();
            _records.remove(&seq).unwrap()
        };
        let rtt = (time_now - last_time + duration) / 2.0;
        logger.write_all(
            format!("{} {:.6}\n", seq, rtt).as_bytes()
        ).unwrap();
    }
}

impl RttRecorder {
    pub fn new(name:&String, port:u16) -> Self {
        let name = name.clone();
        let port = port + 1024; //pong recv port
        let record_handle = None;
        let recv_handle = None;
        RttRecorder{ name, port, record_handle, recv_handle }
    }

    pub fn start(&mut self) -> mpsc::Sender<u32> {
        let (tx, rx) = mpsc::channel::<u32>();
        let (name, port) = (self.name.clone(), self.port);
        let records1: GuardRttRecords = Arc::new(Mutex::new(HashMap::new()));
        let records2 = records1.clone();

        self.record_handle = Some(
            thread::spawn(move || { record_thread(rx, records1); })
        );
        self.recv_handle = Some(
            thread::spawn(move || { pong_recv_thread(name, port, records2); } )
        );

        tx
    }

    
}
