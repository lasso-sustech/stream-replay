use std::thread::{JoinHandle, yield_now};
use std::sync::{Arc, Mutex, mpsc};
// use std::time::Duration;

use crate::packet::{PacketStruct, PacketSender, PacketReceiver, tos2ac};
use crate::dispatcher::{UdpDispatcher, SourceInput};

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
            // thread::sleep( Duration::from_micros(10) );
        }
    }
}

pub struct GlobalBroker {
    name: Option<String>,
    pub dispatcher: UdpDispatcher,
    apps: [GuardedApplications; 4]
}

impl GlobalBroker {
    pub fn new(name:Option<String>) -> Self {
        let apps = [
            Arc::new(Mutex::new( Vec::<Application>::new() )),
            Arc::new(Mutex::new( Vec::<Application>::new() )),
            Arc::new(Mutex::new( Vec::<Application>::new() )),
            Arc::new(Mutex::new( Vec::<Application>::new() ))
        ];
        let dispatcher = UdpDispatcher::new();

        Self { name, dispatcher, apps }
    }

    pub fn add(&mut self, ipaddr:String, tos: u8, priority: String) -> SourceInput {
        match self.name {
            None => {
                // let (tx, rx) = mpsc::channel::<PacketStruct>();
                self.dispatcher.start_new(ipaddr, tos)
            }
            Some(_) => {
                let (tx, broker_rx) = mpsc::channel::<PacketStruct>();
                let (broker_tx, blocked_signal) = self.dispatcher.start_new(ipaddr, tos);

                let conn = (broker_rx, broker_tx);
                let app = Application{ conn, priority };
                let ac = tos2ac( tos );
                self.apps[ac].lock().unwrap().push( app );

                (tx, blocked_signal)
            }
        }
    }

    pub fn append(&mut self, tos: u8, priority: String) -> SourceInput {
        let ac = tos2ac( tos );
        match self.name {
            None => {
                let (tx, blocked_signal) = self.dispatcher.records.get(ac).unwrap();
                ( tx.clone(), Arc::clone(&blocked_signal) )
            }
            Some(_) => {
                let (tx, broker_rx) = mpsc::channel::<PacketStruct>();
                let (broker_tx, blocked_signal) = self.dispatcher.records.get(ac).unwrap();
                let (broker_tx, blocked_signal) = (
                    broker_tx.clone(), Arc::clone(&blocked_signal) );

                let conn = (broker_rx, broker_tx);
                let app = Application{ conn, priority };
                self.apps[ac].lock().unwrap().push( app );

                (tx, blocked_signal)
            }
        }
    }

    pub fn start(&self) -> JoinHandle<()> {
        let apps:Vec<_> = self.apps.iter().map(|app| app.clone()).collect();

        std::thread::spawn(move || {
            policy_priority_fifo(apps);
        })
    }

}
