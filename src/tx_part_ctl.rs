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

    // ------------------------->  Channel 0
    //   Channel 1  <-----------------------
    // 50, 49, ..., 15, 14, 13, 12,..., 1, 0
    //              ^           ^
    //              |           |
    //         tx_part_ch1  tx_part_ch0
    pub fn get_packet_state(&self, offset: u16, num: usize) -> (bool, bool, bool) {
        let tx_part_ch0 = self.tx_parts[0] * num as f64;
        let tx_part_ch1 = self.tx_parts[1] * num as f64;
        let offset = offset as f64;
        let ch0 = offset >= tx_part_ch0;
        let ch1 = offset < tx_part_ch1;
        if ch0 && (offset >= tx_part_ch0 && offset < tx_part_ch0 + 1.0) {
            return (ch0, ch1, true)
        }
        if ch1 && (offset < tx_part_ch1 && offset >= tx_part_ch1 - 1.0) {
            return (ch0, ch1, true)
        }
        return (ch0, ch1, false)
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
            let port = packet.port;
            let ips = self.port2ip.get(&port).unwrap();
            let tx_ipaddr = ips[1].clone();
            if PacketStruct::channel_info(packet.indicators) == 0 {
                if part_packets.contains_key(&tx_ipaddr) {
                    part_packets.get_mut(&tx_ipaddr).unwrap().push(packet.clone());
                } else {
                    part_packets.insert(tx_ipaddr, vec![packet.clone()]);
                }
            }
        }

        // Process packets for the second part
        for packet in packets.iter_mut().rev() {
            let port = packet.port;
            let ips = self.port2ip.get(&port).unwrap();
            let tx_ipaddr = ips[1].clone();
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
