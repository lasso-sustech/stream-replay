mod conf;
mod throttle;

use std::fs::File;
use std::io::prelude::*;
use std::collections::VecDeque;

use std::path::Path;
use std::thread;
use std::sync::mpsc;
use std::net::UdpSocket;
use std::time::{SystemTime, Duration};

use clap::Parser;
use serde_json;
use rand::prelude::*;
use ndarray::prelude::*;
use ndarray_npy::read_npy;
use std::os::unix::io::AsRawFd;

use crate::conf::{Manifest, StreamParam, ConnParams};
use crate::throttle::{RateThrottle};

const UDP_MAX_LENGTH:usize = 1500 - 20 - 8;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about=None)]
struct ProgArgs {
    /// The manifest file tied with the data trace.
    #[clap( value_parser )]
    manifest_file: String,
    /// The target server IP address.
    #[clap( value_parser )]
    target_ip_address: String,
}

const MAX_PAYLOAD_LEN:usize = UDP_MAX_LENGTH - 8;
#[repr(C,packed)]
#[derive(Copy, Clone, Debug)]
struct PacketStruct {
    seq: u32,//4 Byte
    offset: u16,// 2 Byte
    length: u16,//2 Byte
    payload: [u8; MAX_PAYLOAD_LEN]
}
impl PacketStruct {
    fn new() -> Self {
        PacketStruct { seq: 0, offset: 0, length: 0, payload: [32u8; MAX_PAYLOAD_LEN] }
    }
    fn set_length(&mut self, length: u16) {
        self.length = length;
    }
    fn next_seq(&mut self, num: usize, remains:usize) {
        self.seq += 1;
        self.offset = if remains>0 {num as u16+1} else {num as u16};
    }
    fn next_offset(&mut self) {
        self.offset -= 1;
    }
}

fn load_trace(param: ConnParams, window_size:usize) -> (Array2<u64>, u16, u8, RateThrottle) {
    let trace: Array2<u64> = read_npy(&param.npy_file).unwrap();
    let port = param.port.unwrap();
    let tos = param.tos.unwrap_or(0);
    let throttle = param.throttle.unwrap_or(0.0);
    let throttle = RateThrottle::new(throttle, window_size);

    (trace, port, tos, throttle)
}

unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    ::std::slice::from_raw_parts(
        (p as *const T) as *const u8,
        ::std::mem::size_of::<T>(),
    )
}

unsafe fn set_tos(fd: i32, tos: u8) -> bool {
    let value = &(tos as i32) as *const libc::c_int as *const libc::c_void;
    let option_len = std::mem::size_of::<libc::c_int>() as u32;
    let res = libc::setsockopt(fd, libc::IPPROTO_IP, libc::IP_TOS, value, option_len);

    res == 0
}

fn producer_thread(tx: mpsc::Sender<PacketStruct>, trace: Array2<u64>, start_offset:usize, mut throttle: RateThrottle) {
    let mut packet = PacketStruct::new();
    let mut idx = start_offset;
    let spin_sleeper = spin_sleep::SpinSleeper::new(10_000)
                        .with_spin_strategy(spin_sleep::SpinStrategy::YieldThread);

    loop {
        idx = (idx + 1) % trace.shape()[0];
        let size_bytes = trace[[idx, 1]] as usize;
        let interval_ns = trace[[idx, 0]];

        // 1. throttle
        while throttle.exceeds_with(size_bytes, interval_ns) {
            std::thread::sleep( Duration::from_nanos(10_000) );
        }
        // 2. send
        let (_num, _remains) = (size_bytes/UDP_MAX_LENGTH, size_bytes%UDP_MAX_LENGTH);
        packet.next_seq(_num, _remains);
        packet.set_length(UDP_MAX_LENGTH as u16);
        for _ in 0.._num {
            tx.send( packet.clone() ).unwrap();
            packet.next_offset();
        }
        if _remains>0 {
            packet.next_offset();
            packet.set_length(_remains as u16);
            tx.send( packet.clone() ).unwrap();
        }
        // 3. wait
        spin_sleeper.sleep( Duration::from_nanos(interval_ns) );
    }
}

fn consumer_thread(rx: mpsc::Receiver<PacketStruct>, addr:String, tos:u8) -> Result<(), std::io::Error> {
    let mut fifo = VecDeque::new();
    let mut file = File::create("data/log.txt")?;

    // create UDP socket
    let sock = UdpSocket::bind("0.0.0.0:0")?;
    let fd = sock.as_raw_fd();
    unsafe{ set_tos(fd, tos); }
    // connect to server
    sock.connect(addr)?;
    
    loop {
        let timestamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64();
        // try to get new packet
        if let Ok(packet) = rx.try_recv() {
            fifo.push_back(packet);
            file.write_all( format!("{:.9} {}\n", timestamp, fifo.len()).as_bytes() )?;
        }
        // try to send packet
        if let Some(packet) = fifo.get(0) {
            let length = packet.length as usize;
            let buf = unsafe{ any_as_u8_slice(packet) };
            sock.send(&buf[..length])?;
            fifo.pop_front();
            file.write_all( format!("{:.9} {}\n", timestamp, fifo.len()).as_bytes() )?;
        }
    }
}

fn main() {
    let mut rng = rand::thread_rng();

    // read the manifest file
    let args = ProgArgs::parse();
    let file = std::fs::File::open(&args.manifest_file).unwrap();
    let reader = std::io::BufReader::new( file );

    let root = Path::new(&args.manifest_file).parent();
    let manifest:Manifest = serde_json::from_reader(reader).unwrap();
    let window_size = manifest.window_size;
    let streams: Vec<_> = manifest.streams.into_iter().filter_map( |x| x.validate(root) ).collect();
    println!("Sliding Window Size: {}.", window_size);

    // spawn the thread
    let mut handles:Vec<_> = streams.into_iter().enumerate().map(|(i, param)| {
        let start_offset: usize = rng.gen();
        let target_address = args.target_ip_address.clone();
        let (StreamParam::UDP(ref params) | StreamParam::TCP(ref params)) = param;
        let (trace, port, tos, throttle) = load_trace(params.clone(), window_size);

        let (tx, rx) = mpsc::channel::<PacketStruct>();
        let producer = thread::spawn(move || {
            producer_thread(tx, trace, start_offset, throttle);
        });
        let consumer = thread::spawn(move || {
            let addr = format!("{}:{}", target_address, port); 
            consumer_thread(rx, addr, tos)
        });

        println!("{}. {} on ...", i+1, param);
        (producer, consumer)
    }).collect();

    //wait on the last consumer handle (maybe panic)
    handles.remove( handles.len()-1 ).1.join().unwrap().unwrap();
}
