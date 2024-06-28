mod packet;
mod destination;

use std::sync::{mpsc, Arc, Mutex};
use clap::Parser;
use crate::destination::*;

fn main() {
    let args = Args::parse();
    let recv_data = Arc::new(Mutex::new(RecvData::new()));
    let recv_data_final = Arc::clone(&recv_data);
    
    let (tx, _rx) = mpsc::channel::<Vec<u8>>();
    recv_data.lock().unwrap().tx = Some(tx);

    // Extract duration from args
    let duration = args.duration;
    
    let lock = Arc::new(Mutex::new(false));
    let lock_clone = Arc::clone(&lock);
    
    std::thread::spawn(move || {
        recv_thread(args, recv_data, lock_clone);
    });

    while !*lock.lock().unwrap() {
        std::thread::sleep(std::time::Duration::from_nanos(100_000) );
    }

    // Sleep for the duration
    std::thread::sleep(std::time::Duration::from_secs(duration as u64));

    let data_len = recv_data_final.lock().unwrap().data_len;
    let rx_duration = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64() - recv_data_final.lock().unwrap().rx_start_time;

    println!("Received Bytes: {:.3} MB", data_len as f64/ 1024.0 / 1024.0);
    println!("Average Throughput: {:.3} Mbps", data_len as f64 / rx_duration / 1e6 * 8.0);
}

