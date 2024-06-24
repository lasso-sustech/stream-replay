mod conf;
mod packet;
mod throttle;
mod broker;
mod source;
mod dispatcher;
mod rtt;
mod socket;
mod ipc;
mod tx_part_ctl;
mod rx;
mod android;

use std::collections::HashMap;
use std::path::Path;
// use std::rc::Rc;

use clap::Parser;
use serde_json;

use crate::conf::Manifest;
use crate::conf::StreamParam;
use crate::ipc::IPCDaemon;
use crate::source::SourceManager;
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
    // #[clap( value_parser )]
    // tx_ip_address: String,
    /// The duration of test procedure (unit: seconds).
    #[clap( value_parser )]
    duration: f64,
    /// IPC Port for real-time access
    #[clap(long, default_value_t = 11112)]
    ipc_port: u16,
}

fn main() {
    // load the manifest file
    let args = ProgArgs::parse();
    let target_ipaddr = args.target_ip_address;
    // let tx_ipaddr = args.tx_ip_address;
    let file = std::fs::File::open(&args.manifest_file).unwrap();
    let reader = std::io::BufReader::new( file );
    let root = Path::new(&args.manifest_file).parent();
    let manifest:Manifest = serde_json::from_reader(reader).unwrap();
    // parse the manifest file
    let streams:Vec<_> = manifest.streams.into_iter().filter_map( |x| x.validate(root, args.duration) ).collect();
    let window_size = manifest.window_size;
    let orchestrator = manifest.orchestrator;
    println!("Sliding Window Size: {}.", window_size);
    println!("Orchestrator: {:?}.", orchestrator);

    // from manifest create a mapping from port to target addr
    let mut port2ip: HashMap<u16, Vec<String>> = HashMap::new();
    for stream in &streams {
        let (port, ip) = match stream {
            StreamParam::TCP(param) => (param.port.clone(), param.tx_ipaddrs.clone()),
            StreamParam::UDP(param) => (param.port.clone(), param.tx_ipaddrs.clone())
        };
        port2ip.insert(port, ip);
    }

    // start broker
    let mut broker = GlobalBroker::new( orchestrator, target_ipaddr, port2ip);
    let _handle = broker.start();

    // spawn the source thread
    let mut sources:HashMap<_,_> = streams.into_iter().map(|stream| {
        let src = SourceManager::new(stream, window_size, &mut broker);
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
