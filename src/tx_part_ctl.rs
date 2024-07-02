use std::collections::HashMap;
use std::vec;
use crate::packet::PacketStruct;
use crate::packet::PacketType;
use crate::link::Link;

#[derive(Debug)]
pub struct TxPartCtler {
    tx_parts: Vec<f64>,
    tx_ipaddrs: Vec<String>,
}

impl TxPartCtler {
    pub fn new(tx_parts: Vec<f64>, links: Vec<Link>) -> Self {
        let mut tx_ipaddrs = Vec::new();
        for link in links.iter() {
            tx_ipaddrs.push(link.tx_ipaddr.clone());
        }
        if tx_parts.len() != tx_ipaddrs.len() {
            panic!("tx_parts and tx_ipaddrs must have the same length");
        }
        TxPartCtler {
            tx_parts,
            tx_ipaddrs,
        }
    }

    pub fn set_tx_parts(&mut self, tx_parts: Vec<f64>) {
        self.tx_parts = tx_parts;
    }

    //   Channel 1  <-----------------------
    // ------------------------->  Channel 0
    // 0, 1, ..., 12, 13, 14, 15,..., 49, 50
    //             ^           ^
    //             |           |
    //        tx_part_ch1  tx_part_ch0
    pub fn get_packet_state(&self, offset: u16, num: usize) -> Vec<PacketType> {
        if self.tx_parts.len() < 2 {
            if offset == (num as u16 - 1) {
                return vec![PacketType::SL];
            }
            return vec![PacketType::SNL];
        }

        let tx_part_ch0 = self.tx_parts[0] * num as f64;
        let tx_part_ch1 = self.tx_parts[1] * num as f64;

        if ( 0 as f64 >= tx_part_ch1) || ( ( (num - 1) as f64 ) < tx_part_ch0  ) {
            if offset == (num as u16 - 1) {
                return vec![PacketType::SL];
            }
            return vec![PacketType::SNL];
        }

        let offset = offset as f64;
        let is_ch0 = offset <= tx_part_ch0;
        let is_ch1 = offset >  tx_part_ch1;
        let is_last_ch0 = offset > (tx_part_ch0 - 1.0);
        let is_first_ch1= offset == num as f64 - 1.0;
        let is_last_ch1 = offset <= (tx_part_ch1 + 1.0);
        
        let mut type_vec = Vec::<PacketType>::new();
        if let Some(packet_type) = match (is_ch0, is_last_ch0) {
            (true, false) => Some(PacketType::DFN),
            (true, true) => Some(PacketType::DFL),
            (false, _) => None,
        } {
            type_vec.push(packet_type);
        }
        if let Some(packet_type) = match (is_ch1, is_last_ch1, is_first_ch1) {
            (true, false, false) => Some(PacketType::DSM),
            (true, true, false) => Some(PacketType::DSL),
            (true, false, true) => Some(PacketType::DSF),
            (true, true, true) => Some(PacketType::DSS),
            _ => None,
        } {
            type_vec.push(packet_type);
        }

        type_vec
        
    }

    pub fn get_packet_states(&self, num: usize) -> Vec<Vec<PacketType>> {
        let mut results = Vec::new();
        for offset in 0..=num-1 {
            let state = self.get_packet_state(offset as u16, num);
            results.push(state);
        }
        results
    }

    pub fn process_packets(&self, mut packets: Vec<PacketStruct>) -> HashMap::<String, Vec<PacketStruct>> {
        let mut part_packets = HashMap::<String, Vec<PacketStruct>>::new();
        
        // Process packets for the first part
        // TODO: modify transimission order for sinlge packet case
        let mut i = 0;
        while i < packets.len() {
            let tx_ipaddr = self.tx_ipaddrs[0].clone();
            match PacketStruct::channel_info(packets[i].indicators) {
                0 => {
                    let packet = packets.remove(i);
                    part_packets.entry(tx_ipaddr)
                        .or_insert_with(Vec::new)
                        .push(packet);
                }
                _ => {
                    i += 1;
                }
            }
        }

        if self.tx_parts.len() < 2 {
            return part_packets;
        }

        // Process packets for the second part
        let mut i = packets.len();
        while i > 0 {
            i -= 1;
            let tx_ipaddr = self.tx_ipaddrs[1].clone();
            match PacketStruct::channel_info(packets[i].indicators) {
                _ => {
                    let packet = packets.remove(i);
                    part_packets.entry(tx_ipaddr)
                        .or_insert_with(Vec::new)
                        .push(packet);
                }
            }
        }
        part_packets
    }
}
