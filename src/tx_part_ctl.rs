use std::collections::HashMap;
use crate::packet::PacketStruct;
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

    // ------------------------->  Channel 0
    //   Channel 1  <-----------------------
    // 50, 49, ..., 15, 14, 13, 12,..., 1, 0
    //              ^           ^
    //              |           |
    //         tx_part_ch1  tx_part_ch0
    pub fn get_packet_state(&self, offset: u16, num: usize) -> (bool, bool, bool) {
        if self.tx_parts.len() < 2 {
            if offset == 0 {
                return (true, false, true)
            }
            return (true, false, false)
        }

        let tx_part_ch0 = self.tx_parts[0] * num as f64;
        let tx_part_ch1 = self.tx_parts[1] * num as f64;
        let offset = offset as f64;
        let is_ch0_p = offset >= tx_part_ch0;
        let is_ch1_p = offset < tx_part_ch1;
        if is_ch0_p && (offset >= tx_part_ch0 && offset < tx_part_ch0 + 1.0) {
            return (is_ch0_p, is_ch1_p, true)
        }
        if is_ch1_p && (offset < tx_part_ch1 && offset >= tx_part_ch1 - 1.0) {
            return (is_ch0_p, is_ch1_p, true)
        }
        return (is_ch0_p, is_ch1_p, false)
    }

    pub fn get_packet_states(&self, num: usize) -> Vec<(bool, bool, bool)> {
        let mut results = Vec::new();
        for offset in (0..=num-1).rev() {
            let state = self.get_packet_state(offset as u16, num);
            results.push(state);
        }
        results
    }

    pub fn process_packets(&self, mut packets: Vec<PacketStruct>) -> HashMap::<String, Vec<PacketStruct>> {
        let mut part_packets = HashMap::<String, Vec<PacketStruct>>::new();
        for packet in packets.iter_mut() {
            let tx_ipaddr = self.tx_ipaddrs[0].clone();
            if PacketStruct::channel_info(packet.indicators) == 0 {
                if part_packets.contains_key(&tx_ipaddr) {
                    part_packets.get_mut(&tx_ipaddr).unwrap().push(packet.clone());
                } else {
                    part_packets.insert(tx_ipaddr, vec![packet.clone()]);
                }
            }
        }

        if self.tx_parts.len() < 2 {
            return part_packets;
        }
        // Process packets for the second part
        for packet in packets.iter_mut().rev() {
            let tx_ipaddr = self.tx_ipaddrs[1].clone();
            if PacketStruct::channel_info(packet.indicators) == 1 {
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
