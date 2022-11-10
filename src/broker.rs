use std::thread::{self, JoinHandle, yield_now};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use crate::packet::{PacketStruct, PacketSender, PacketReceiver, tos2ac};

type SourceSink = (PacketSender, PacketReceiver);
type BrokerConn = (PacketReceiver, PacketSender);
struct Application {
    conn: BrokerConn,
    priority: String
}
type GuardedApplications = Arc<Mutex<Vec<Application>>>;

fn policy_priority_fifo(apps: Vec<GuardedApplications>) {
    let passthrough = |app:&Application| {
        let _ = app.priority;
        for packet in app.conn.0.try_iter() {
            if let Err(_) = app.conn.1.send(packet) {
                return false;
            }
        }
        true
    };

    loop {
        for app in apps.iter() {
            yield_now();
            app.lock().unwrap().retain(passthrough);
            thread::sleep( Duration::from_micros(50) );
        }
    }
}

pub struct GlobalBroker {
    apps: [GuardedApplications; 4]
}

impl GlobalBroker {
    pub fn new() -> Self {
        let apps = [
            Arc::new(Mutex::new( Vec::<Application>::new() )),
            Arc::new(Mutex::new( Vec::<Application>::new() )),
            Arc::new(Mutex::new( Vec::<Application>::new() )),
            Arc::new(Mutex::new( Vec::<Application>::new() ))
        ];
        Self { apps }
    }

    pub fn add(&mut self, tos: u8, priority: String) -> SourceSink {
        let (tx, broker_rx) = mpsc::channel::<PacketStruct>();
        let (broker_tx, rx) = mpsc::channel::<PacketStruct>();

        let conn = (broker_rx, broker_tx);
        let app = Application{ conn, priority };
        let ac = tos2ac( tos );
        self.apps[ac].lock().unwrap().push( app );

        (tx, rx)
    }

    pub fn start(&self) -> JoinHandle<()> {
        let apps:Vec<_> = self.apps.iter().map(|app| app.clone()).collect();

        std::thread::spawn(move || {
            policy_priority_fifo(apps);
        })
    }

}
