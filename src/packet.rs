use std::sync::mpsc;

pub const UDP_MAX_LENGTH:usize = 1500 - 20 - 10;
const MAX_PAYLOAD_LEN:usize = UDP_MAX_LENGTH - 10;

pub type PacketSender   = mpsc::Sender<PacketStruct>;
pub type PacketReceiver = mpsc::Receiver<PacketStruct>;

#[repr(C,packed)]
#[derive(Copy, Clone, Debug)]
pub struct PacketStruct {
    seq: u32,//4 Bytes
    offset: u16,//2 Bytes
    pub length: u16,//2 Bytes
    pub port: u16,//2 Bytes
    payload: [u8; MAX_PAYLOAD_LEN]
}

impl PacketStruct {
    pub fn new(port: u16) -> Self {
        PacketStruct { seq: 0, offset: 0, length: 0, port, payload: [32u8; MAX_PAYLOAD_LEN] }
    }
    pub fn set_length(&mut self, length: u16) {
        self.length = length;
    }
    pub fn next_seq(&mut self, num: usize, remains:usize) {
        self.seq += 1;
        self.offset = if remains>0 {num as u16+1} else {num as u16};
    }
    pub fn next_offset(&mut self) {
        self.offset -= 1;
    }
}

//Reference: https://wireless.wiki.kernel.org/en/developers/documentation/mac80211/queues
pub fn tos2ac(tos: u8) -> usize {
    let ac_bits = (tos & 0xE0) >> 5;
    match ac_bits {
        0b001 | 0b010 => 3, // AC_BK (AC3)
        0b000 | 0b011 => 2, // AC_BE (AC2)
        0b100 | 0b101 => 1, // AC_VI (AC1)
        0b110 | 0b111 => 0, // AC_VO (AC0)
        _ => { panic!("Impossible ToS value.") }
    }
}
