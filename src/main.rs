mod conf;
mod packet;
mod throttle;
mod broker;

use std::path::Path;
use std::thread;
use std::sync::{Arc, Mutex, mpsc};
use std::net::UdpSocket;
use std::time::{Duration, SystemTime};

use clap::Parser;
use serde_json;
use rand::prelude::*;
use ndarray::prelude::*;
use ndarray_npy::read_npy;
use std::os::unix::io::AsRawFd;

use crate::conf::{Manifest, StreamParam, ConnParams};
use crate::packet::*;
use crate::throttle::RateThrottler;
use crate::broker::GlobalBroker;

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

type BlockedSignal = Arc<Mutex<bool>>;

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

fn load_trace(param:ConnParams, window_size:usize) -> Option<(Array2<u64>, u16, u8, RateThrottler, String)> {
    let trace: Array2<u64> = read_npy(&param.npy_file).ok()?;
    let port = param.port?;
    let tos = param.tos.unwrap_or(0);

    let throttle = param.throttle.unwrap_or(0.0);
    let _name = format!("{}:{}", port, tos);
    let throttler = RateThrottler::new(_name, throttle, window_size);
    
    let priority = param.priority.unwrap_or( "".into() );

    Some((trace, port, tos, throttler, priority))
}

fn source_thread(tx:PacketSender, trace:Array2<u64>, start_offset:usize, port:u16, mut throttler:RateThrottler, blocked_signal:BlockedSignal) {
    let mut packet = PacketStruct::new(port);
    let mut idx = start_offset;
    let spin_sleeper = spin_sleep::SpinSleeper::new(100_000)
                        .with_spin_strategy(spin_sleep::SpinStrategy::YieldThread);

    loop {
        idx = (idx + 1) % trace.shape()[0];
        let size_bytes = trace[[idx, 1]] as usize;
        let interval_ns = trace[[idx, 0]];
        let deadline = SystemTime::now() + Duration::from_nanos(interval_ns);

        // 1. generate packets
        let mut packets = Vec::new();
        let (_num, _remains) = (size_bytes/UDP_MAX_LENGTH, size_bytes%UDP_MAX_LENGTH);
        packet.next_seq(_num, _remains);
        packet.set_length(UDP_MAX_LENGTH as u16);
        for _ in 0.._num {
            packets.push( packet.clone() );
            packet.next_offset();
        }
        if _remains > 0 {
            packet.next_offset();
            packet.set_length(_remains as u16);
            packets.push( packet.clone() );
        }
        throttler.prepare( packets );

        // 2. send aware of blocked status
        while SystemTime::now() < deadline {
            let _signal = blocked_signal.lock().unwrap();
            if !(*_signal) {
                match throttler.try_consume(|packet| {
                    tx.send(packet).unwrap();
                    true
                }) {
                    Some(_) => continue,
                    None=> break
                }
            }
        }

        // 3. sleep until next arrival
        if let Ok(remaining_time) = deadline.duration_since( SystemTime::now() ) {
            spin_sleeper.sleep( remaining_time );
        }
    }
}

fn sink_thread(rx: PacketReceiver, addr:String, tos:u8, blocked_signal:BlockedSignal) -> Result<(), std::io::Error> {
    // create UDP socket
    let sock = UdpSocket::bind("0.0.0.0:0")?;
    sock.set_nonblocking(true).unwrap();
    unsafe {
        let fd = sock.as_raw_fd();
        assert!( set_tos(fd, tos) );
    }
    // connect to server
    sock.connect(addr)?;
    
    loop {
        // fetch bulky packets
        let packets:Vec<_> = rx.try_iter().collect();
        // send bulky packets aware of block status
        for packet in packets.iter() {
            let length = packet.length as usize;
            let buf = unsafe{ any_as_u8_slice(packet) };
            loop {
                let mut _signal = blocked_signal.lock().unwrap();
                match sock.send(&buf[..length]) {
                    Ok(_len) => {
                        *_signal = false;
                        break
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        *_signal = true;
                        continue // block occurs
                    }
                    Err(e) => panic!("encountered IO error: {e}")
                }
            }             
        }
    }
}

fn main() {
    let mut rng = rand::thread_rng();
    let mut broker = GlobalBroker::new();
    let _handle = broker.start();

    // read the manifest file
    let args = ProgArgs::parse();
    let file = std::fs::File::open(&args.manifest_file).unwrap();
    let reader = std::io::BufReader::new( file );

    let root = Path::new(&args.manifest_file).parent();
    let manifest:Manifest = serde_json::from_reader(reader).unwrap();
    let streams: Vec<_> = manifest.streams.into_iter().filter_map( |x| x.validate(root) ).collect();
    let window_size = manifest.window_size;
    let orchestrator = manifest.orchestrator;
    println!("Sliding Window Size: {}.", window_size);
    println!("Orchestrator: {:?}.", orchestrator);

    // spawn the thread
    let mut handles:Vec<_> = streams.into_iter().enumerate().map(|(i, param)| {
        let start_offset: usize = rng.gen();
        let target_address = args.target_ip_address.clone();
        let (StreamParam::UDP(ref params) | StreamParam::TCP(ref params)) = param;

        // define block signal
        let blocked_signal:BlockedSignal = Arc::new(Mutex::new(false));

        // add to broker
        let (trace, port, tos, throttler, priority) = load_trace(params.clone(), window_size)
                .expect( &format!("{} loading failed.", param) );
        let (tx, rx) = match orchestrator {
            Some(_) => broker.add(tos, priority),
            None=> mpsc::channel::<PacketStruct>()
        };
        
        // spawn source and sink threads
        let cloned_blocked_signal = Arc::clone(&blocked_signal);
        let source = thread::spawn(move || {
            source_thread(tx, trace, start_offset, port, throttler, Arc::clone(&blocked_signal))
        });
        let sink = thread::spawn(move || {
            let addr = format!("{}:{}", target_address, port); 
            sink_thread(rx, addr, tos, cloned_blocked_signal)
        });

        println!("{}. {} on ...", i+1, param);
        (source, sink)
    }).collect();

    //TODO: block on the exit of last sink thread
    //wait on the first sink handle (maybe panic)
    handles.remove( 0 ).1.join().unwrap().unwrap();
}
