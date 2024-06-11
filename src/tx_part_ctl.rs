use std::collections::HashMap;
use crate::packet::PacketStruct;

#[derive(Debug)]
pub struct TxPartCtler {
    tx_parts: Vec<f64>,
    port2ip: HashMap<u16, Vec<String>>,
}

impl TxPartCtler {
    pub fn new(tx_parts: Vec<f64>, port2ip: HashMap<u16, Vec<String>>) -> Self {
        TxPartCtler {
            tx_parts,
            port2ip,
        }
    }

    pub fn set_tx_parts(&mut self, tx_parts: Vec<f64>) {
        self.tx_parts = tx_parts;
    }

    pub fn process_packets(&self, mut packets: Vec<PacketStruct>) -> HashMap::<String, Vec<PacketStruct>> {
        let mut part_packets = HashMap::<String, Vec<PacketStruct>>::new();
        println!("tx_parts: {:?}", self.tx_parts);
        for packet in packets.iter_mut() {
            let port = packet.port;
            let num = packet.num as f64;

            let ips = self.port2ip.get(&port).unwrap();
            let tx_part_ch0 = self.tx_parts[0] * num;
            let offset = packet.offset as f64;
            packet.set_indicator(0);

            if !self.tx_parts.is_empty() && offset >= tx_part_ch0 {
                if offset >= tx_part_ch0 && offset < (tx_part_ch0 + 1.0) {
                    packet.set_indicator(10);
                }
                let tx_ipaddr = ips[0].clone();
                if part_packets.contains_key(&tx_ipaddr) {
                    part_packets.get_mut(&tx_ipaddr).unwrap().push(packet.clone());
                } else {
                    part_packets.insert(tx_ipaddr, vec![packet.clone()]);
                }
            }
        }

        // Process packets for the second part
        for packet in packets.iter_mut().rev() {
            packet.set_indicator(1);
            let port = packet.port;
            let num = packet.num as f64;

            let ips = self.port2ip.get(&port).unwrap();
            let tx_part_ch1 = self.tx_parts[1] * num;
            let offset = packet.offset as f64;
            packet.set_indicator(1);

            if self.tx_parts.len() > 1 && offset < tx_part_ch1 {
                if offset < tx_part_ch1 && offset >= (tx_part_ch1 - 1.0) {
                    packet.set_indicator(11);
                }
                let tx_ipaddr = ips[1].clone();
                if part_packets.contains_key(&tx_ipaddr) {
                    part_packets.get_mut(&tx_ipaddr).unwrap().push(packet.clone());
                } else {
                    part_packets.insert(tx_ipaddr, vec![packet.clone()]);
                }
            }
        }
        part_packets
    }
}
