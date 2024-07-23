use std::collections::HashMap;
use std::sync:: mpsc;
use std::thread::{self};
use std::net::ToSocketAddrs;
use std::time::SystemTime;
use log::trace;

use crate::link::Link;

use stream_replay::core::packet::{self, any_as_u8_slice, PacketReceiver, PacketStruct, APP_HEADER_LENGTH};
use stream_replay::core::socket::{*};
use std::net::UdpSocket;

pub fn dispatch(links: Vec<Link>, tos:u8) -> HashMap<String, mpsc::Sender<PacketStruct>> {
    // create Hashmap for each tx_ipaddr and set each non blocking
    let mut socket_infos = HashMap::new();

    let mut handles = Vec::new();
    for link in links.iter() {
        let tx_ipaddr = link.tx_ipaddr.clone();
        let rx_addr =  format!("{}:0",link.rx_ipaddr.clone()).to_socket_addrs().unwrap().next().unwrap();
        let socket = create_udp_socket(tos, tx_ipaddr.clone());
        let (socket_tx, socket_rx) = mpsc::channel::<PacketStruct>();
        if let Some(socket) = socket {
            socket.set_nonblocking(true).unwrap();
            socket_infos.insert(tx_ipaddr.clone(),  socket_tx );
            let _handle = thread::spawn(move || {
                let socket = socket.try_clone().unwrap();
                socket_thread(socket, socket_rx, rx_addr);
            });
            handles.push(_handle);
        }
        else{
            eprintln!("Socket creation failure: ip_addr {} tos {}.", tx_ipaddr, tos);
            break;
        }
    }
    socket_infos
}

fn socket_thread(sock: UdpSocket, rx:PacketReceiver, mut addr:std::net::SocketAddr) {
    loop {
        let packets:Vec<_> = rx.try_iter().collect();
        if packets.len()==0 {
            // std::thread::sleep( Duration::from_nanos(10_000) );
            continue;
        }
        for packet in packets.iter() {
            let length = APP_HEADER_LENGTH + packet.length as usize;
            let buf = unsafe{ any_as_u8_slice(packet) };
            loop {
                addr.set_port( packet.port );
                match sock.send_to(&buf[..length], &addr) {
                    Ok(_len) => {
                        match packet::get_packet_type(packet.indicators) {
                            packet::PacketType::SL | packet::PacketType::DSL | packet::PacketType::DFL => {
                                trace!("Socket: Time {} -> seq {}-offset {}-ip_addr {}", SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64() , packet.seq as u32, packet.offset as u16, addr);
                            }
                            _ => {}
                        }
                        break
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // std::thread::sleep( Duration::from_nanos(10_000) );
                        continue // block occurs
                    }
                    Err(e) => panic!("encountered IO error: {e}")
                }
            } 
        }   
    }
}