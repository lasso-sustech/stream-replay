use std::fs::File;
use std::io::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self,};
use std::time::Duration;

type LogMessage = (&'static str,String,String);
type GuardedReceiver = Arc<Mutex<mpsc::Receiver<LogMessage>>>;
type GuardedMap = Arc<Mutex<HashMap<String,File>>>;
pub type LogProxy = mpsc::Sender<LogMessage>;
pub type GuardedLogHub = Arc<Mutex<LoggingHub>>;
pub struct LoggingHub {
    tx: LogProxy,
    rx: GuardedReceiver,
    map: GuardedMap
}

fn logging_thread(rx: GuardedReceiver, map: GuardedMap) {
    while let Ok((prefix, name, message)) = rx.lock().unwrap().recv() {
        if let Ok(map) = map.lock() {
            let _name = format!("{}-{}", prefix, name);
            let mut logger = map.get(&_name).unwrap();
            logger.write_all(message.as_bytes()).unwrap();
        }
    }
}

impl LoggingHub {
    pub fn new() -> Self {
        let (tx,rx) = mpsc::channel::<LogMessage>();
        let rx = Arc::new(Mutex::new(rx));
        let map = Arc::new(Mutex::new(HashMap::new()));
        Self{ tx, rx, map }
    }

    pub fn register(&mut self, prefix:&str, name:&str) -> LogProxy {
        let _name = format!("{}-{}", prefix, name);
        let path_name = format!("logs/{}.txt", _name);
        let logger = File::create(path_name).unwrap();
        let mut _map = self.map.lock().unwrap();
        _map.insert( String::from(_name), logger );
        self.tx.clone()
    }

    pub fn start(&self) {
        let (rx, map) = (Arc::clone(&self.rx), Arc::clone(&self.map));
        thread::spawn(move || { logging_thread(rx, map) });
    }
}

type GuardedRate = Arc<Mutex<f64>>;
type RateRecords = Vec<GuardedRate>;
type GuardedRateRecords = Arc<Mutex<RateRecords>>;
pub struct RateWatch {
    vec_rate: GuardedRateRecords
}

fn rate_watch_thread(vec_rate: GuardedRateRecords) {
    loop {
        thread::sleep( Duration::from_millis(200) );
        if let Ok(vec_rate) = vec_rate.try_lock() {
            let _results:Vec<_> = vec_rate.iter().enumerate().map(|(i,rate)| {
                let _rate = rate.lock().unwrap();
                format!("({}) {:.3} Mbps", i+1, _rate)
            }).collect();
            print!("\r{:?}", _results);
            std::io::stdout().flush().unwrap_or(());
        }
    }
}

impl RateWatch {
    pub fn new() -> Self {
        let vec_rate = Arc::new(Mutex::new( Vec::new() ));
        Self{ vec_rate }
    }

    pub fn register(&mut self, ref_rate:&GuardedRate) -> Option<()> {
        let mut _vec_rate = self.vec_rate.lock().ok()?;
        Some( _vec_rate.push( Arc::clone(ref_rate) ) )
    }

    pub fn start(&self) {
        let _vec_rate = self.vec_rate.clone();
        thread::spawn(move|| {rate_watch_thread(_vec_rate)});
    }
}
