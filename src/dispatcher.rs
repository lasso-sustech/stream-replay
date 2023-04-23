use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::net::ToSocketAddrs;
use crate::packet::{PacketSender,PacketReceiver, PacketStruct, APP_HEADER_LENGTH, any_as_u8_slice};
use crate::socket::*;

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

    pub fn start_new(&mut self, ipaddr:String, tos:u8) -> SourceInput {
        let (tx, rx) = crossbeam_channel::unbounded::<PacketStruct>();
        let blocked_signal:BlockedSignal = Arc::new(Mutex::new(false));
        let cloned_blocked_signal = Arc::clone(&blocked_signal);
        let handle = thread::spawn(move || {
            dispatcher_thread(rx, ipaddr, tos, blocked_signal)
        });

        let res = ( tx.clone(), Arc::clone(&cloned_blocked_signal) );
        self.records.push( (tx, cloned_blocked_signal) );
        self.handles.push( handle );

        res
    }

    pub fn start_agg_sockets(&mut self, ipaddr:String) {
        let ipaddr_list = std::iter::repeat(ipaddr);
        let tos_list = [192, 128, 96, 32];
        let _:Vec<_> = std::iter::zip(ipaddr_list, tos_list).map(
            |(ipaddr, tos)| {
                self.start_new(ipaddr, tos)
            }
        ).collect(); //discard the cloned responses
    }

}

fn dispatcher_thread(rx: PacketReceiver, ipaddr:String, tos:u8, blocked_signal:BlockedSignal) {
    let mut addr = format!("{}:0", ipaddr).to_socket_addrs().unwrap().next().unwrap();

    if let Some(sock) = create_udp_socket(tos) {
        sock.set_nonblocking(true).unwrap();

        loop {
            // fetch bulky packets
            let mut packets:Vec<PacketStruct> = Vec::with_capacity( rx.len() );
            packets.extend( rx.try_iter() );
            // send bulky packets aware of blocking status
            for packet in packets.iter() {
                let length = packet.length as usize;
                let length = std::cmp::max(length, APP_HEADER_LENGTH);
                let buf = unsafe{ any_as_u8_slice(packet) };
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
    else {
        let packets:Vec<_> = rx.try_iter().collect();
        for packet in packets.iter() {
            let port = packet.port;
            eprintln!("Socket creation failure: {}:{}@{}.", ipaddr, port, tos);
            break;
        }
    }
}
