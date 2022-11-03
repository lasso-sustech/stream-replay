use std::thread::{JoinHandle, yield_now};
use std::sync::{Arc, Mutex, mpsc};

use crate::{PacketStruct, PacketSender, PacketReceiver};

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
        
        //reference: https://wireless.wiki.kernel.org/en/developers/documentation/mac80211/queues
        let ac_bits = (tos & 0xE0) >> 5;
        match ac_bits {
            // AC_BK (AC3)
            0b001 | 0b010 => { self.apps[3].lock().unwrap().push(app); }
            // AC_BE (AC2)
            0b000 | 0b011 => { self.apps[2].lock().unwrap().push(app); }
            // AC_VI (AC1)
            0b100 | 0b101 => { self.apps[1].lock().unwrap().push(app); }
            // AC_VO (AC0)
            0b110 | 0b111 => { self.apps[0].lock().unwrap().push(app); }
            // otherwise
            _ => { panic!("Impossible ToS value.") }
        }

        (tx, rx)
    }

    pub fn start(&self) -> JoinHandle<()> {
        let apps:Vec<_> = self.apps.iter().map(|app| app.clone()).collect();
        

        std::thread::spawn(move || {
            policy_priority_fifo(apps);
        })
    }

}
