use std::thread::{JoinHandle, sleep, yield_now};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use crate::conf::ConnParams;
use crate::packet::{PacketStruct, PacketSender, PacketReceiver, tos2ac};
use crate::dispatcher::{UdpDispatcher, SourceInput};

type BrokerConn = (PacketReceiver, PacketSender);
struct Application {
    conn: BrokerConn,
    priority: String
}
type GuardedApplications = Arc<Mutex<Vec<Application>>>;

fn policy_priority_fifo(all_apps: Vec<GuardedApplications>) {
    let passthrough = |app:&Application| {
        let _ = app.priority;
        for packet in app.conn.0.try_iter() {
            if let Err(_) = app.conn.1.send(packet) {
                return false;
            }
        }
        if app.priority.contains("guarded,") {
            sleep( Duration::from_micros(10) );
        }
        true
    };

    loop {
        for ac_apps in all_apps.iter() {
            yield_now();
            ac_apps.lock().unwrap().retain(passthrough);
            sleep( Duration::from_micros(10) );
        }
    }
}

pub struct GlobalBroker {
    name: Option<String>,
    pub dispatcher: UdpDispatcher,
    apps: [GuardedApplications; 4]
}

impl GlobalBroker {
    pub fn new(name:Option<String> ) -> Self {
        let apps = [
            Arc::new(Mutex::new( Vec::<Application>::new() )),
            Arc::new(Mutex::new( Vec::<Application>::new() )),
            Arc::new(Mutex::new( Vec::<Application>::new() )),
            Arc::new(Mutex::new( Vec::<Application>::new() ))
        ];
        let dispatcher = UdpDispatcher::new();

        Self { name, dispatcher, apps }
    }

    pub fn add(&mut self, param: ConnParams) -> SourceInput {
        let tos = param.tos;
        let priority = param.priority.clone();
        let links = param.links.clone();
        let tx_parts = param.tx_parts.clone();
        let ac = tos2ac( param.tos );

        let (broker_tx, blocked_signal, tx_part_ctl) = {
            self.dispatcher.start_new(
                links,
                tos, 
                tx_parts
            )
        };

        match self.name {
            None => (broker_tx, blocked_signal, tx_part_ctl),
            Some(_) => {
                let (tx, broker_rx) = mpsc::channel::<PacketStruct>();

                let conn = (broker_rx, broker_tx);
                let app = Application{ conn, priority };
                self.apps[ac].lock().unwrap().push( app );

                (tx, blocked_signal, tx_part_ctl)
            }
        }
    }

    pub fn start(&mut self) -> JoinHandle<()> {
        let apps:Vec<_> = self.apps.iter().map(|app| app.clone()).collect();

        std::thread::spawn(move || {
            policy_priority_fifo(apps);
        })
    }

}
