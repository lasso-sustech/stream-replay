use std::collections::HashMap;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::net::ToSocketAddrs;
use std::time::Duration;
use crate::packet::{PacketSender,PacketReceiver, PacketStruct, APP_HEADER_LENGTH, any_as_u8_slice};
use crate::socket::{*};

pub type SourceInput = (PacketSender, BlockedSignal);
pub type BlockedSignal = Arc<Mutex<bool>>;

pub struct UdpDispatcher {
    pub records: Vec<SourceInput>,
    handles: Vec<JoinHandle<()>>
}

impl UdpDispatcher {
    pub fn new() -> Self {
        let records = Vec::new();
        let handles = Vec::new();

        Self { records, handles }
    }

    pub fn start_new(&mut self, ipaddr:String, tos:u8, tx_ipaddrs: Vec<String>, port2ip: HashMap<u16, Vec<String>>) -> SourceInput {
        let (tx, rx) = mpsc::channel::<PacketStruct>();
        let blocked_signal:BlockedSignal = Arc::new(Mutex::new(false));
        let cloned_blocked_signal = Arc::clone(&blocked_signal);
        let handle = thread::spawn(move || {
            dispatcher_thread(rx, ipaddr, tos, blocked_signal, tx_ipaddrs.clone(), port2ip.clone())
        });

        let res = ( tx.clone(), Arc::clone(&cloned_blocked_signal) );
        self.records.push( (tx, cloned_blocked_signal) );
        self.handles.push( handle );

        res
    }

    pub fn start_agg_sockets(&mut self, ipaddr:String, tx_ipaddrs: Vec<String>, port2ip: HashMap<u16, Vec<String>>) {
        let ipaddr_list = std::iter::repeat(ipaddr);
        let tos_list = [192, 128, 96, 32];
        let _:Vec<_> = std::iter::zip(ipaddr_list, tos_list).map(
            |(ipaddr, tos)| {
                self.start_new(ipaddr, tos, tx_ipaddrs.clone(), port2ip.clone())
            }
        ).collect(); //discard the cloned responses
    }

}

fn dispatcher_thread(rx: PacketReceiver, ipaddr:String, tos:u8, blocked_signal:BlockedSignal, tx_ipaddrs:Vec<String>, port2ip:HashMap<u16, Vec<String>>) {
    let mut addr = format!("{}:0", ipaddr).to_socket_addrs().unwrap().next().unwrap();

    // create Hashmap for each tx_ipaddr and set each non blocking
    let mut sockets = HashMap::new();

    for tx_ipaddr in tx_ipaddrs.iter() {
        let socket = create_udp_socket(tos, tx_ipaddr.clone());
        if let Some(socket) = socket {
            socket.set_nonblocking(true).unwrap();
            sockets.insert(tx_ipaddr.clone(), socket);
        }
        else{
            let packets:Vec<_> = rx.try_iter().collect();
            for packet in packets.iter() {
                let port = packet.port;
                eprintln!("Socket creation failure: {}:{}@{}.", ipaddr, port, tos);
                break;
            }
        }
    }

    loop {
        // fetch bulky packets
        let packets:Vec<_> = rx.try_iter().collect();
        if packets.len()==0 {
            std::thread::sleep( Duration::from_nanos(10_000) );
            continue;
        }
        // send bulky packets aware of blocking status
        for packet in packets.iter() {
            let length = packet.length as usize;
            let port = packet.port;
            let ips = port2ip.get(&port).unwrap();
            let length = std::cmp::max(length, APP_HEADER_LENGTH);
            let buf = unsafe{ any_as_u8_slice(packet) };
            // select socket for transmission based on packet seq and length
            let tx_ipaddr = ips[ (packet.offset as usize) % ips.len() ].clone(); // round robin
            let sock = sockets.get(&tx_ipaddr).unwrap();
            loop {
                addr.set_port( packet.port );
                let mut _signal = blocked_signal.lock().unwrap();
                match sock.send_to(&buf[..length], &addr) {
                    Ok(_len) => {
                        *_signal = false;
                        break
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        *_signal = true;
                        continue // block occurs
                    }
                    Err(e) => panic!("encountered IO error: {e}")
                }
            }             
        }
    }
}
