use crate::packet::{self, PacketType};
use crate::link::Link;

type OffsetPacket = (u16, PacketType);
#[derive(Debug)]
pub struct TxPartCtler {
    pub tx_parts: Vec<f64>,
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

    fn is_single_channel(&self, num: usize) -> bool {
        if self.tx_parts.len() < 2 {
            return true;
        }

        let tx_part_ch0 = self.tx_parts[0] * num as f64;
        let tx_part_ch1 = self.tx_parts[1] * num as f64;

        tx_part_ch1 <= 0.0 || ((num - 1) as f64) < tx_part_ch0
    }

    pub fn get_packet_state(&self, offset: f64, num: usize, channel: u16) -> Option<PacketType> {
        match channel {
            1 => {
                let tx_part_ch0 = self.tx_parts[0] * num as f64;
                match (offset < tx_part_ch0, offset >= tx_part_ch0 - 1.0) {
                    (true, false) => Some(PacketType::DFN),
                    (true, true) => Some(PacketType::DFL),
                    _ => None,
                }
            },
            2 => {
                let tx_part_ch1 = self.tx_parts[1] * num as f64;
                match (offset >= tx_part_ch1, offset < tx_part_ch1 + 1.0, offset == (num - 1) as f64) {
                    (true, false, false) => Some(PacketType::DSM),
                    (true, true, false) => Some(PacketType::DSL),
                    (true, false, true) => Some(PacketType::DSF),
                    (true, true, true) => Some(PacketType::DSS),
                    _ => None,
                }
            },
            _ => {
                if offset == (num - 1) as f64 {
                    Some(PacketType::SL)
                } else {
                    Some(PacketType::SNL)
                }
            }
        }
    }

    pub fn get_packet_states(&self, num: usize) -> Vec<Vec<OffsetPacket>> {
        let mut results = vec![Vec::new(); 3];
    
        if self.is_single_channel(num) {
            for offset in 0..num {
                if let Some(packet_type) = self.get_packet_state(offset as f64, num, 0) {
                    results[0].push((offset as u16, packet_type));
                }
            }
        } else {
            for offset in 0..num {
                if let Some(packet_type) = self.get_packet_state(offset as f64, num, 1)  {
                    results[1].push((offset as u16, packet_type));
                }
                if let Some(packet_type) = self.get_packet_state(offset as f64, num, 2) {
                    results[2].push((offset as u16, packet_type));
                }
            }
        }
        // inverse results[2]
        // results[2].reverse();
        results
    }

    pub fn packet_to_ipaddr(&self, indicator: u8) -> String {

        let channel = {
            if self.tx_parts.len() >= 2 &&self.tx_parts[0] <= 0.0 {
                1 as u8
            }
            else {
                packet::channel_info(indicator)
            }
        };

        self.tx_ipaddrs[channel as usize].clone()
    }
}