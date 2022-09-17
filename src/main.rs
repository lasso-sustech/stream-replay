mod conf;

use std::path::{Path};
use clap::Parser;
use serde_json;
use rand::prelude::*;
use ndarray::prelude::*;
use ndarray_npy::read_npy;
use tokio::net::{TcpSocket, UdpSocket};
use tokio::time::{sleep, Duration};
use tokio::io::AsyncWriteExt;

use crate::conf::{StreamParam, ConnParams};

const UDP_MAX_LENGTH:usize = 65507;
const TCP_MAX_LENGTH:usize = 65535;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about=None)]
struct ProgArgs {
    /// The manifest file tied with the data trace.
    #[clap( value_parser )]
    manifest_file: String,
    #[clap( value_parser )]
    target_ip_address: String
}

fn load_trace(param: ConnParams) -> (Array2<u64>, u16, u8) {
    let trace: Array2<u64> = read_npy(&param.npy_file).unwrap();
    let port = param.port.unwrap();
    let tos = param.tos.unwrap();

    (trace, port, tos)
}

async fn send_loop(target_address:String, start_offset:usize, param: StreamParam) -> Result<(), std::io::Error> {
    let buffer = [32u8; 500*1024];

    match param {
        StreamParam::UDP(param) => {
            let (trace, port, tos) = load_trace(param);
            // create UDP socket
            let sock = UdpSocket::bind("0.0.0.0:12345").await?;
            sock.set_tos(tos as u32).unwrap();
            // connect to server
            // sock.connect( format!("{}:{}",target_address,port) ).await?;
            let addr = format!("{}:{}",target_address,port);
            let mut idx = start_offset;
            loop {
                idx = (idx + 1) % trace.shape()[0];
                
                let len = trace[[idx, 1]] as usize;
                for i in (0..len).step_by(UDP_MAX_LENGTH) {
                    let _rng = i..std::cmp::min(i+UDP_MAX_LENGTH, len);
                    let _len = sock.send_to(&buffer[_rng], addr.clone()).await?;
                    println!("[UDP] {:?} bytes sent", _len);
                }

                sleep( Duration::from_nanos(trace[[idx, 0]]) ).await;
            }
        }
        StreamParam::TCP(param) => {
            let (trace, port, tos) = load_trace(param);
            // create TCP socket
            let sock = TcpSocket::new_v4()?;
            sock.set_tos(tos as u32).unwrap();
            // connect to server
            let addr = format!("{}:{}",target_address,port).parse().unwrap();
            let mut stream = sock.connect( addr ).await?;
            let mut idx = start_offset;
            loop {
                idx = (idx + 1) % trace.shape()[0];

                let len = trace[[1, idx]] as usize;
                for i in (0..len).step_by(TCP_MAX_LENGTH) {
                    let _rng = i..std::cmp::min(i+TCP_MAX_LENGTH, len);
                    let _len = stream.write_all(&buffer[_rng]).await?;
                    println!("[TCP] {:?} bytes sent", _len);
                }

                sleep( Duration::from_nanos(trace[[idx, 0]]) ).await;
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
    let streams: Vec<StreamParam> = serde_json::from_reader(reader).unwrap();
    let streams: Vec<_> = streams.into_iter().filter_map( |x| x.validate(root) ).collect();

    // spawn the thread
    let mut handles:Vec<_> = streams.into_iter().map(|param| {
        let start_offset: usize = rng.gen();
        let target_address = args.target_ip_address.clone();
        tokio::spawn(async move {
            send_loop(target_address, start_offset, param).await
        })
    }).collect();

    //wait on the last handle (maybe panic)
    handles.remove( handles.len()-1 ).await.unwrap().unwrap()
}
