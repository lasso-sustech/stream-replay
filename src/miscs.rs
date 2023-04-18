use std::fs::File;
use std::io::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self,};

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
