use std::collections::HashMap;

use std::sync::mpsc::Sender;
use stream_replay::core::packet::{self, PacketStruct, PacketType};

#[derive(Default)]
struct RecvOffsets {
    sl: Option<u16>,
    dfl: Option<u16>,
    dsf: Option<u16>,
    dsl: Option<u16>,
}

#[derive(Default)]
struct RecvComplete {
    sl_complete: bool,
    ch1_complete: bool,
    ch2_complete: bool,
}

type IsACK = (bool, bool);

pub struct RecvRecord {
    pub packets: HashMap<u16, PacketStruct>, // Use a HashMap to store packets by their offset
    pub is_ack: IsACK,
    offsets: RecvOffsets,
    is_complete: RecvComplete,
}

impl RecvRecord {
    pub fn new() -> Self{
        Self{
            packets: HashMap::<u16, PacketStruct>::new(),
            is_ack : (false, false),
            offsets: RecvOffsets::default(),
            is_complete: RecvComplete::default(),
        }
    }
    pub fn record(&mut self, data: &[u8]) {
        let packet = packet::from_buffer(data);
        let offset = Some(packet.offset);

        match packet::get_packet_type(packet.indicators) {
            PacketType::SL  => self.offsets.sl = offset,
            PacketType::DFL => self.offsets.dfl = offset,
            PacketType::DSF => self.offsets.dsf = offset,
            PacketType::DSL => self.offsets.dsl = offset,
            PacketType::DSS => {
                self.offsets.dsf = offset;
                self.offsets.dsl = offset;
            }
            _ => {}
        }

        self.packets.insert(packet.offset as u16, packet);
        self.is_complete = self.determine_complete();
    }

    pub fn is_complete(&self) -> bool {
        self.is_complete.sl_complete || (self.is_complete.ch1_complete && self.is_complete.ch2_complete)
    }

    pub fn is_fst_ack(&self) -> bool {
        !self.is_ack.0 && (self.is_complete.sl_complete || self.is_complete.ch1_complete)
    }

    pub fn is_scd_ack(&self) -> bool {
        !self.is_ack.1 && self.is_complete.ch2_complete
    }

    fn determine_complete(&self) -> RecvComplete {
        fn is_range_complete(packets: &HashMap<u16, PacketStruct>, mut range: std::ops::RangeInclusive<u16>) -> bool {
            range.all(|i| packets.contains_key(&i))
        }
    
        if let Some(sl) = self.offsets.sl {
            if is_range_complete(&self.packets, 0..=sl) {
                return RecvComplete { sl_complete: true, ch1_complete: false, ch2_complete: false };
            }
        }
    
        let ch1_complete = self.offsets.dfl.map_or(false, |dfl| is_range_complete(&self.packets, 0..=dfl));
        let ch2_complete = match (self.offsets.dsf, self.offsets.dsl) {
            (Some(dsf), Some(dsl)) => is_range_complete(&self.packets, dsl..=dsf),
            _ => false,
        };

        RecvComplete { sl_complete: false, ch1_complete, ch2_complete }
    }
    #[allow(dead_code)]
    pub fn gather(&self) -> Vec<u8>{
        let mut data = Vec::new();
        let num_packets = self.packets.len();
        for i in 0..num_packets{
            let packet = self.packets.get(&(i as u16)).unwrap();
            data.extend_from_slice(&packet.payload[ ..packet.length as usize]);
        }
        return data;
    }
}

pub struct RecvData{
    pub recv_records: HashMap<u32, RecvRecord>,
    pub last_seq: u32,
    pub recevied: u32,
    pub data_len: u32,
    pub rx_start_time: f64,
    pub tx: Option<Sender<Vec<u8>>>
}

impl RecvData{
    pub fn new() -> Self{
        Self{
            recv_records: HashMap::new(),
            last_seq: 0,
            recevied: 0,
            data_len: 0,
            rx_start_time: 0.0,
            tx: None,
        }
    }
}
