mod conf;
mod throttle;

use std::path::{Path};
use clap::Parser;
use serde_json;
use rand::prelude::*;
use ndarray::prelude::*;
use ndarray_npy::read_npy;
use tokio::net::{TcpSocket, UdpSocket};
use tokio::time::{sleep, Duration};
use tokio::io::AsyncWriteExt;

use crate::conf::{Manifest, StreamParam, ConnParams};
use crate::throttle::{RateThrottle};

const UDP_MAX_LENGTH:usize = 1500 - 20 - 8;
const TCP_MAX_LENGTH:usize = 65535;

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
struct PacketStruct {
    seq: u32,//4 Byte
    offset: u16,// 2 Byte
    payload: [u8; MAX_PAYLOAD_LEN]
}
impl PacketStruct {
    fn new() -> Self {
        PacketStruct { seq: 0, offset: 0, payload: [32u8; MAX_PAYLOAD_LEN] }
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

async fn send_loop(target_address:String, start_offset:usize, window_size:usize, param: StreamParam) -> Result<(), std::io::Error> {
    let mut packet = PacketStruct::new();

    match param {
        StreamParam::UDP(param) => {
            let (trace, port, tos, mut throttle) = load_trace(param, window_size);
            // create UDP socket
            let sock = UdpSocket::bind("0.0.0.0:0").await?;
            sock.set_tos(tos as u32).unwrap();
            // connect to server
            // sock.connect( format!("{}:{}",target_address,port) ).await?;
            let addr = format!("{}:{}",target_address,port);
            let mut idx = start_offset;
            loop {
                idx = (idx + 1) % trace.shape()[0];
                let size_bytes = trace[[idx, 1]] as usize;
                let interval_ns = trace[[idx, 0]];
                // 1. throttle
                while throttle.exceeds_with(size_bytes, interval_ns) {
                    sleep( Duration::from_micros(1) ).await;
                }
                // 2. send
                let (_num, _remains) = (size_bytes/UDP_MAX_LENGTH, size_bytes%UDP_MAX_LENGTH);
                packet.seq += 1;
                packet.offset = if _remains>0 {_num as u16+1} else {_num as u16};
                for _ in 0.._num {
                    let buffer = unsafe{ any_as_u8_slice(&packet) };
                    let _len = sock.send_to(&buffer[..UDP_MAX_LENGTH], addr.clone()).await?;
                    packet.offset -= 1;
                }
                if _remains>0 {
                    packet.offset = 0;
                    let buffer = unsafe{ any_as_u8_slice(&packet) };
                    let _len = sock.send_to(&buffer[.._remains], addr.clone()).await?;
                }
                // println!("[UDP] {:?} bytes sent", _len);
                // 3. wait
                sleep( Duration::from_nanos(interval_ns) ).await;
            }
        }
        StreamParam::TCP(param) => {
            let (trace, port, tos, mut throttle) = load_trace(param, window_size);
            // create TCP socket
            let sock = TcpSocket::new_v4()?;
            sock.set_tos(tos as u32).unwrap();
            // connect to server
            let addr = format!("{}:{}",target_address,port).parse().unwrap();
            let mut stream = sock.connect( addr ).await?;
            let mut idx = start_offset;
            loop {
                idx = (idx + 1) % trace.shape()[0];
                let size_bytes = trace[[idx, 1]] as usize;
                let interval_ns = trace[[idx, 0]];
                // 1. throttle
                while throttle.exceeds_with(size_bytes, interval_ns) {
                    sleep( Duration::from_micros(1) ).await;
                }
                // 2. send
                for i in (0..size_bytes).step_by(TCP_MAX_LENGTH).rev() {
                    let _rng = 0..std::cmp::min(i+TCP_MAX_LENGTH, size_bytes)-i;
                    let buffer = unsafe{ any_as_u8_slice(&packet) };
                    let _len = stream.write_all(&buffer[_rng]).await?;
                    // println!("[TCP] {:?} bytes sent", _len);
                }
                // 3. wait
                sleep( Duration::from_nanos(interval_ns) ).await;
            }
        }
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
