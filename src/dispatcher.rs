use std::collections::HashMap;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::net::ToSocketAddrs;
use std::time::{Duration, SystemTime};
use log::trace;

use crate::link::Link;

use crate::packet::{PacketSender,PacketReceiver, PacketStruct, APP_HEADER_LENGTH, any_as_u8_slice};
use crate::socket::{*};
use crate::tx_part_ctl::TxPartCtler;
use std::net::UdpSocket;

pub type SourceInput = (PacketSender, BlockedSignal, Arc<Mutex<TxPartCtler>>);
pub type BlockedSignal = Arc<Mutex<bool>>;

pub struct UdpDispatcher {
    handles: Vec<JoinHandle<()>>,
}

impl UdpDispatcher {
    pub fn new() -> Self {
        let handles = Vec::new();
        Self { handles}
    }

    pub fn start_new(&mut self, links: Vec<Link>, tos:u8, tx_parts: Vec<f64>) -> SourceInput {
        let (tx, rx) = mpsc::channel::<PacketStruct>();
        let blocked_signal:BlockedSignal = Arc::new(Mutex::new(false));
        let cloned_blocked_signal = Arc::clone(&blocked_signal);
        let tx_part_ctler = Arc::new(Mutex::new(TxPartCtler::new( tx_parts.clone(), links.clone() )));
        let cloned_tx_part_ctler = Arc::clone(&tx_part_ctler);
        let handle = thread::spawn(move || {
            dispatcher_thread(rx, links, tos, blocked_signal,  tx_part_ctler)
        });

        let res = ( tx.clone(), Arc::clone(&cloned_blocked_signal), cloned_tx_part_ctler );
        self.handles.push( handle );
        res
    }

}

fn dispatcher_thread(rx: PacketReceiver, links:Vec<Link>, tos:u8, blocked_signal:BlockedSignal,  tx_part_ctler:Arc<Mutex<TxPartCtler>>) {
    // create Hashmap for each tx_ipaddr and set each non blocking
    let mut socket_infos = HashMap::new();

    let mut handles = Vec::new();
    for link in links.iter() {
        let tx_ipaddr = link.tx_ipaddr.clone();
        let rx_addr =  format!("{}:0",link.rx_ipaddr.clone()).to_socket_addrs().unwrap().next().unwrap();
        let blocked_signal = Arc::clone(&blocked_signal);
        let socket = create_udp_socket(tos, tx_ipaddr.clone());
        let (socket_tx, socket_rx) = mpsc::channel::<PacketStruct>();
        if let Some(socket) = socket {
            socket.set_nonblocking(true).unwrap();
            socket_infos.insert(tx_ipaddr.clone(),  socket_tx );
            let _handle = thread::spawn(move || {
                let socket = socket.try_clone().unwrap();
                socket_thread(socket, socket_rx, blocked_signal, rx_addr);
            });
            handles.push(_handle);
        }
        else{
            let packets:Vec<_> = rx.try_iter().collect();
            for packet in packets.iter() {
                let port = packet.port;
                eprintln!("Socket creation failure: {}:{}@{}.", tx_ipaddr, port, tos);
                break;
            }
        }
    }
    // packet sender
    loop {
        // fetch bulky packets
        let packets:Vec<_> = rx.try_iter().collect();
        if packets.len()==0 {
            std::thread::sleep( Duration::from_nanos(10_000) );
            continue;
        }
        // prepare packets for each tx_ipaddr
        tx_part_ctler.lock().unwrap().process_packets(packets)
        .iter()
        .for_each(|(tx_ipaddr, packets)| {
            if let Some(socket_tx) = socket_infos.get(tx_ipaddr) {
                for packet in packets.iter() {
                    trace!("Dispatcher: Time {} -> seq {}-offset {}-ip_addr {}", SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64() , packet.seq as u32, packet.offset as u16, tx_ipaddr);
                    socket_tx.send(packet.clone()).unwrap();
                }
            }
        });
    }
}


fn socket_thread(sock: UdpSocket, rx:PacketReceiver, blocked_signal:BlockedSignal, mut addr:std::net::SocketAddr) {
    loop {
        let packets:Vec<_> = rx.try_iter().collect();
        if packets.len()==0 {
            std::thread::sleep( Duration::from_nanos(10_000) );
            continue;
        }
        for packet in packets.iter() {
            let length = APP_HEADER_LENGTH + packet.length as usize;
            let buf = unsafe{ any_as_u8_slice(packet) };
            loop {
                addr.set_port( packet.port );
                let mut _signal = blocked_signal.lock().unwrap();
                match sock.send_to(&buf[..length], &addr) {
                    Ok(_len) => {
                        *_signal = false;
                        trace!("Socket: Time {} -> seq {}-offset {}-ip_addr {}", SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64() , packet.seq as u32, packet.offset as u16, addr);
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