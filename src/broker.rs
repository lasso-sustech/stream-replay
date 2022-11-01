use std::{sync::{Arc, Mutex, mpsc}, thread::JoinHandle};
use crate::{PacketStruct, PacketSender, PacketReceiver};

type SourceSink = (PacketSender, PacketReceiver);
type BrokerConn = (PacketReceiver, PacketSender);
struct Application {
    conn: BrokerConn,
    priority: String
}

pub struct GlobalBroker {
    ac0_apps: Arc<Mutex<Vec<Application>>>,
    ac1_apps: Arc<Mutex<Vec<Application>>>,
    ac2_apps: Arc<Mutex<Vec<Application>>>,
    ac3_apps: Arc<Mutex<Vec<Application>>>,
}

impl GlobalBroker {
    pub fn new() -> Self {
        Self {
            ac0_apps: Arc::new(Mutex::new( Vec::<Application>::new() )),
            ac1_apps: Arc::new(Mutex::new( Vec::<Application>::new() )),
            ac2_apps: Arc::new(Mutex::new( Vec::<Application>::new() )),
            ac3_apps: Arc::new(Mutex::new( Vec::<Application>::new() ))
        }
    }

    pub fn add(&mut self, tos: u8, priority: String) -> SourceSink {
        let (tx, broker_rx) = mpsc::channel::<PacketStruct>();
        let (broker_tx, rx) = mpsc::channel::<PacketStruct>();

        let conn = (broker_rx, broker_tx);
        let app = Application{ conn, priority };
        
        let ac_bits = (tos & 0xE0) >> 5;
        match ac_bits {
            // AC_BK (AC3)
            0b001 | 0b010 => { self.ac3_apps.lock().unwrap().push(app); }
            // AC_BE (AC2)
            0b000 | 0b011 => { self.ac2_apps.lock().unwrap().push(app); }
            // AC_VI (AC1)
            0b100 | 0b101 => { self.ac1_apps.lock().unwrap().push(app); }
            // AC_VO (AC0)
            0b110 | 0b111 => { self.ac0_apps.lock().unwrap().push(app); }
            // otherwise
            _ => { panic!("Impossible ToS value.") }
        }

        (tx, rx)
    }

    pub fn remove() {

    }

    pub fn start(&self) -> JoinHandle<()> {
        let ac0_apps = Arc::clone(&self.ac0_apps);
        let ac1_apps = Arc::clone(&self.ac1_apps);
        let ac2_apps = Arc::clone(&self.ac2_apps);
        let ac3_apps = Arc::clone(&self.ac3_apps);

        std::thread::spawn(move || loop {
            for app in ac0_apps.lock().unwrap().iter() {
                let _ = app.priority;
                if let Ok(packet) = app.conn.0.try_recv() {
                    app.conn.1.send(packet).unwrap();
                }
            }

            for app in ac1_apps.lock().unwrap().iter() {
                if let Ok(packet) = app.conn.0.try_recv() {
                    app.conn.1.send(packet).unwrap();
                }
            }

            for app in ac2_apps.lock().unwrap().iter() {
                if let Ok(packet) = app.conn.0.try_recv() {
                    app.conn.1.send(packet).unwrap();
                }
            }

            for app in ac3_apps.lock().unwrap().iter() {
                if let Ok(packet) = app.conn.0.try_recv() {
                    app.conn.1.send(packet).unwrap();
                }
            }
        })
    }

}
