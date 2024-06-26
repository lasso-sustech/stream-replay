#![allow(dead_code)]
use std::sync::mpsc;

const IP_HEADER_LENGTH:usize = 20;
const UDP_HEADER_LENGTH:usize = 8;
pub const APP_HEADER_LENGTH:usize = 19;
pub const UDP_MAX_LENGTH:usize = 1500 - IP_HEADER_LENGTH - UDP_HEADER_LENGTH;
pub const MAX_PAYLOAD_LEN:usize = UDP_MAX_LENGTH - APP_HEADER_LENGTH;

pub type PacketSender   = mpsc::Sender<PacketStruct>;
pub type PacketReceiver = mpsc::Receiver<PacketStruct>;

pub unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    ::std::slice::from_raw_parts(
        (p as *const T) as *const u8,
        ::std::mem::size_of::<T>(),
    )
}

pub enum PacketType {
    SNL,
    SL,
    DFN,
    DFL,
    DSS,
    DSF,
    DSM,
    DSL
}

#[repr(C,packed)]
#[derive(Copy, Clone, Debug)]
pub struct PacketStruct {
    pub seq: u32,       //4 Bytes
    pub offset: u16,    //2 Bytes, how much left to send
    pub length: u16,    //2 Bytes
    pub port: u16,      //2 Bytes
    pub indicators: u8, //1 Byte, 0 - 1 represents the interface id, 10~19 represents the last packet of interface id 
    pub timestamp: f64, //8 Bytes
    pub payload: [u8; MAX_PAYLOAD_LEN]
}

impl PacketStruct {
    pub fn new(port: u16) -> Self {
        PacketStruct { seq: 0, offset: 0, length: 0, port, timestamp:0.0, indicators:0 , payload: [32u8; MAX_PAYLOAD_LEN] }
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

    pub fn set_indicator(&mut self, packet_type: PacketType) {
        match packet_type {
            PacketType::SNL => self.indicators = 0b00000000,
            PacketType::SL => self.indicators  = 0b00000001,
            PacketType::DFN => self.indicators = 0b00000010,
            PacketType::DFL => self.indicators = 0b00000011,
            PacketType::DSS => self.indicators = 0b00000100,
            PacketType::DSF => self.indicators = 0b00000101,
            PacketType::DSM => self.indicators = 0b00000110,
            PacketType::DSL => self.indicators = 0b00000111,
        }
    }
    pub fn get_packet_type(indicators: u8) -> PacketType {
        match indicators {
            0b00000000 => PacketType::SNL,
            0b00000001 => PacketType::SL,
            0b00000010 => PacketType::DFN,
            0b00000011 => PacketType::DFL,
            0b00000100 => PacketType::DSS,
            0b00000101 => PacketType::DSF,
            0b00000110 => PacketType::DSM,
            0b00000111 => PacketType::DSL,
            _ => panic!("Invalid packet type")
        }
    }

    pub fn channel_info(indicator: u8) -> u8{
        (indicator & 0b00000100) >> 2
    }

    pub fn from_buffer(buffer: &[u8]) -> Self {
        let seq = u32::from_le_bytes(buffer[0..4].try_into().unwrap());
        let offset = u16::from_le_bytes(buffer[4..6].try_into().unwrap());
        let length = u16::from_le_bytes(buffer[6..8].try_into().unwrap());
        let port = u16::from_le_bytes(buffer[8..10].try_into().unwrap());
        let indicators = buffer[10];
        let timestamp = f64::from_le_bytes(buffer[11..19].try_into().unwrap());
        
        let mut payload = [0u8; MAX_PAYLOAD_LEN];
        payload[0..length as usize].copy_from_slice(&buffer[19..length as usize + 19]);

        PacketStruct {
            seq,
            offset,
            length,
            port,
            indicators,
            timestamp,
            payload,
        }
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
