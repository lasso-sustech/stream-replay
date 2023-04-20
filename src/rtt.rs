use std::collections::HashMap;
use std::net::UdpSocket;
use std::thread::{self, JoinHandle};
use std::sync::{Arc, Mutex, mpsc};
use std::time::SystemTime;
use crate::miscs::{LogProxy,GuardedLogHub};

type RttRecords = HashMap<u32,f64>;
type GuardRttRecords = Arc<Mutex<RttRecords>>;

static PONG_PORT_INC:u16 = 1024;

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
        _records.insert(seq, time_now);
    }
}

fn pong_recv_thread(name: String, port: u16, records: GuardRttRecords, logger:LogProxy) {
    let mut buf = [0; 2048];
    let sock = UdpSocket::bind( format!("0.0.0.0:{}", port)).unwrap();

    while let Ok(_) = sock.recv_from(&mut buf) {
        let msg: [u8;4] = buf[..4].try_into().unwrap();
        let seq = u32::from_le_bytes( msg );
        let _msg: [u8;8] = buf[10..18].try_into().unwrap();
        let _duration = f64::from_le_bytes( _msg );
        let time_now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64();
        if let Some(last_time) = {
            let mut _records = records.lock().unwrap();
            _records.remove(&seq)
        } {
            let rtt = time_now - last_time;
            let name = name.clone();
            let message = format!("{} {:.6}\n", seq, rtt);
            logger.send(("rtt", name, message)).unwrap();
        };
    }
}

impl RttRecorder {
    pub fn new(name:&String, port:u16) -> Self {
        let name = name.clone();
        let port = port + PONG_PORT_INC; //pong recv port
        let record_handle = None;
        let recv_handle = None;
        RttRecorder{ name, port, record_handle, recv_handle }
    }

    pub fn start(&mut self, logger:GuardedLogHub) -> mpsc::Sender<u32> {
        let (tx, rx) = mpsc::channel::<u32>();
        let (name, port) = (self.name.clone(), self.port);
        let records1: GuardRttRecords = Arc::new(Mutex::new(HashMap::new()));
        let records2 = records1.clone();

        let logger = logger.lock().unwrap().register("rtt", &name);

        self.record_handle = Some(
            thread::spawn(move || { record_thread(rx, records1); })
        );
        self.recv_handle = Some(
            thread::spawn(move || { pong_recv_thread(name, port, records2, logger); } )
        );

        tx
    }

    
}
