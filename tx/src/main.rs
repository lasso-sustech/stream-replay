mod conf;
mod throttle;
mod source;
mod dispatcher;
mod rtt;
mod ipc;
mod tx_part_ctl;
mod link;
mod statistic;

use std::collections::HashMap;
use std::path::Path;
use log::info;

use core::logger::init_log;
use std::time::SystemTime;
// use std::rc::Rc;

use clap::Parser;
use serde_json;

use crate::conf::Manifest;
use crate::ipc::IPCDaemon;
use crate::source::SourceManager;


#[derive(Parser, Debug)]
#[clap(author, version, about, long_about=None)]
struct ProgArgs {
    /// The manifest file tied with the data trace.
    #[clap( value_parser )]
    manifest_file: String,
    /// The duration of test procedure (unit: seconds).
    #[clap( value_parser )]
    duration: f64,
    /// IPC Port for real-time access
    #[clap(long, default_value_t = 11112)]
    ipc_port: u16,
}

fn main() {
    init_log(false);
    // load the manifest file
    let args = ProgArgs::parse();
    info!{"Starting Transmitting as time {}.", SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64()};
    // let tx_ipaddr = args.tx_ip_address;
    let file = std::fs::File::open(&args.manifest_file).unwrap();
    let reader = std::io::BufReader::new( file );
    let root = Path::new(&args.manifest_file).parent();
    let manifest:Manifest = serde_json::from_reader(reader).unwrap();
    // parse the manifest file
    let streams:Vec<_> = manifest.streams.into_iter().filter_map( |x| x.validate(root, args.duration) ).collect();
    let window_size = manifest.window_size;
    println!("Sliding Window Size: {}.", window_size);

    // spawn the source thread
    let mut sources:HashMap<_,_> = streams.into_iter().map(|stream| {
        let src = SourceManager::new(stream, window_size);
        let name = src.name.clone();
        (name, src)
    }).collect();
    let _handles:Vec<_> = sources.iter_mut().enumerate().map(|(i,(_name,src))| {
        src.start(i+1, String::from("0.0.0.0"))
    }).collect();

    // start global IPC
    let ipc = IPCDaemon::new( sources, args.ipc_port, String::from("0.0.0.0"));
    ipc.start_loop( args.duration);

    std::process::exit(0); //force exit
}
