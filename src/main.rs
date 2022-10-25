mod conf;
mod throttle;

use std::fs::File;
use std::io::prelude::*;
use std::time::SystemTime;
use std::collections::VecDeque;
use std::path::{Path};
use clap::Parser;
use serde_json;
use rand::prelude::*;
use ndarray::prelude::*;
use ndarray_npy::read_npy;
use tokio::io::{self, };
use tokio::net::{UdpSocket};
use tokio::time::{sleep, Duration, sleep_until, Instant};
use tokio::sync::mpsc;

use crate::conf::{Manifest, StreamParam, ConnParams};
use crate::throttle::{RateThrottle};

const UDP_MAX_LENGTH:usize = 1500 - 20 - 8;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about=None)]
struct ProgArgs {
    /// The target server IP address.
    #[clap( value_parser )]
    target_ip_address: String,
    /// The manifest file tied with the data trace.
    #[clap( value_parser )]
    manifest_file: String
}

const MAX_PAYLOAD_LEN:usize = UDP_MAX_LENGTH - 6;
#[repr(C,packed)]
#[derive(Copy, Clone, Debug)]
struct PacketStruct {
    seq: u32,//4 Byte
    offset: u16,// 2 Byte
    payload: [u8; MAX_PAYLOAD_LEN]
}
impl PacketStruct {
    fn new() -> Self {
        PacketStruct { seq: 0, offset: 0, payload: [32u8; MAX_PAYLOAD_LEN] }
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

async fn data_put_thread(tx: mpsc::Sender<PacketStruct>, trace: Array2<u64>, start_offset:usize, mut throttle: RateThrottle) {
    let mut packet = PacketStruct::new();
    let mut idx = start_offset;

    loop {
        idx = (idx + 1) % trace.shape()[0];
        let size_bytes = trace[[idx, 1]] as usize;
        let interval_ns = trace[[idx, 0]];
        let interval_deadline = Instant::now() + Duration::from_nanos(interval_ns);

        // 1. throttle
        while throttle.exceeds_with(size_bytes, interval_ns) {
            sleep( Duration::from_micros(1) ).await;
        }
        // 2. send
        let (_num, _remains) = (size_bytes/UDP_MAX_LENGTH, size_bytes%UDP_MAX_LENGTH);
        packet.next_seq(_num, _remains);
        for _ in 0.._num {
            tx.send( packet.clone() ).await.unwrap();
            packet.next_offset();
        }
        if _remains>0 {
            packet.next_offset();
            tx.send( packet.clone() ).await.unwrap();
        }
        // 3. wait
        sleep_until( interval_deadline ).await;
    }
}

async fn send_loop(target_address:String, start_offset:usize, window_size:usize, param: StreamParam) -> Result<(), std::io::Error> {    
    match param {
        StreamParam::UDP(param) => {
            let (tx, mut rx) = mpsc::channel(1000);
            let mut fifo = VecDeque::new();
            let mut file = File::create("log.txt")?;

            let (trace, port, tos, throttle) = load_trace(param, window_size);
            // create UDP socket
            let sock = UdpSocket::bind("0.0.0.0:0").await?;
            sock.set_tos(tos as u32).unwrap();
            // connect to server
            let addr = format!("{}:{}",target_address,port);
            sock.connect(addr).await?;

            tokio::spawn(async move {
                data_put_thread(tx, trace, start_offset, throttle).await;
            });

            loop {
                let timestamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64();
                // try to get new packet
                if let Ok(packet) = rx.try_recv() {
                    fifo.push_back(packet);
                    file.write_all( format!("{} {}", timestamp, fifo.len()).as_bytes() )?; //NOTE:record
                }
                // try to send packet
                if let Some(packet) = fifo.get(0) {
                    let buffer = unsafe{ any_as_u8_slice(&packet) };
                    match sock.try_send(buffer) {
                        Ok(_len) => {
                            // println!("[UDP] {:?} bytes sent", _len);
                            fifo.pop_front();
                            file.write_all( format!("{} {}", timestamp, fifo.len()).as_bytes() )?; //NOTE: record
                        }
                        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                            // no need to pop
                        }
                        Err(e) => {
                            return Err(e);
                        }
                    }
                }
            }
        
        }
        StreamParam::TCP(_) => { Ok(()) }
    }

    // Ok(())
}

#[tokio::main]
async fn main() {
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
        tokio::spawn(async move {
            println!("{}. {} on ...", i+1, param);
            send_loop(target_address, start_offset, window_size, param).await
        })
    }).collect();

    //wait on the last handle (maybe panic)
    handles.remove( handles.len()-1 ).await.unwrap().unwrap()
}
