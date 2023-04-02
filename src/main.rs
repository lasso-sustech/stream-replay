mod conf;
mod packet;
mod throttle;
mod broker;
mod dispatcher;
mod rtt;
mod socket;

use std::path::Path;
use std::thread;
use std::time::{Duration, SystemTime};

use clap::Parser;
use serde_json;
use rand::prelude::*;
use ndarray::prelude::*;
use ndarray_npy::read_npy;

use crate::conf::{Manifest, StreamParam, ConnParams};
use crate::packet::*;
use crate::throttle::RateThrottler;
use crate::broker::GlobalBroker;
use crate::dispatcher::BlockedSignal;
use crate::rtt::RttRecorder;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about=None)]
struct ProgArgs {
    /// The manifest file tied with the data trace.
    #[clap( value_parser )]
    manifest_file: String,
    /// The target server IP address.
    #[clap( value_parser )]
    target_ip_address: String,
    /// The duration of test procedure (unit: seconds).
    #[clap( value_parser )]
    duration: f64
}

fn load_trace(param:ConnParams, window_size:usize) -> Option<(Array2<u64>, u16, u8, RateThrottler, String, bool)> {
    let trace: Array2<u64> = read_npy(&param.npy_file).ok()?;
    let port = param.port?;
    let tos = param.tos.unwrap_or(0);

    let no_logging = param.no_logging.unwrap_or(false);
    let throttle = param.throttle.unwrap_or(0.0);
    let _name = format!("{}@{}", port, tos);
    let throttler = RateThrottler::new(_name, throttle, window_size, no_logging);
    
    let priority = param.priority.unwrap_or( "".into() );
    let calc_rtt = param.calc_rtt.unwrap_or(false);

    Some((trace, port, tos, throttler, priority, calc_rtt))
}

fn source_thread(tx:PacketSender, trace:Array2<u64>, start_offset:usize, port:u16, mut throttler:RateThrottler, blocked_signal:BlockedSignal, calc_rtt:bool) {
    let mut packet = PacketStruct::new(port);
    let mut idx = start_offset;
    let spin_sleeper = spin_sleep::SpinSleeper::new(100_000)
                        .with_spin_strategy(spin_sleep::SpinStrategy::YieldThread);

    let mut rtt_recorder = RttRecorder::new( &throttler.name, port );
    let rtt_tx = match calc_rtt {
        false => None,
        true => {
            Some( rtt_recorder.start() )
        }
    };

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
        if let Some(ref r_tx) = rtt_tx {
            r_tx.send(packet.seq).unwrap();
        }

        // 2. send aware of blocked status
        while SystemTime::now() < deadline {
            let _signal = blocked_signal.lock().unwrap();
            if !(*_signal) {
                match throttler.try_consume(|mut packet| {
                    let time_now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64();
                    packet.timestamp = time_now;
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

fn main() {
    let mut rng = rand::thread_rng();

    // read the manifest file
    let args = ProgArgs::parse();
    let ipaddr = args.target_ip_address;
    let file = std::fs::File::open(&args.manifest_file).unwrap();
    let reader = std::io::BufReader::new( file );

    let root = Path::new(&args.manifest_file).parent();
    let manifest:Manifest = serde_json::from_reader(reader).unwrap();
    let streams:Vec<_> = manifest.streams.into_iter().filter_map( |x| x.validate(root) ).collect();
    let window_size = manifest.window_size;
    let orchestrator = manifest.orchestrator;
    println!("Sliding Window Size: {}.", window_size);
    println!("Orchestrator: {:?}.", orchestrator);

    // start broker
    let mut broker = GlobalBroker::new( orchestrator, ipaddr, manifest.use_agg_socket );
    let _handle = broker.start();

    // spawn the thread
    let _handles:Vec<_> = streams.into_iter().enumerate().map(|(i, param)| {
        let start_offset: usize = rng.gen();
        let (StreamParam::UDP(ref params) | StreamParam::TCP(ref params)) = param;

        // add to broker, and spawn the corresponding source thread
        let (trace, port, tos, throttler, priority, calc_rtt) = load_trace(params.clone(), window_size)
                .expect( &format!("{} loading failed.", param) );
        let (tx, blocked_signal) = broker.add(tos, priority);
        let source = thread::spawn(move || {
            source_thread(tx, trace, start_offset, port, throttler, blocked_signal, calc_rtt)
        });

        println!("{}. {} on ...", i+1, param);
        source
    }).collect();

    // block on the exit of last source thread
    // let _:Vec<_> = handles.drain(..).map(|handle|{
    //     handle.join().ok();
    // }).collect();

    thread::sleep(Duration::from_secs_f64( args.duration ));
    std::process::exit(0);
}
