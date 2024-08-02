mod destination;
mod record;
mod statistic;

use std::{fs::File, io::Write, sync::{mpsc, Arc, Mutex}};
use clap::Parser;
use record::RecvData;
use crate::destination::*;
use core::logger::init_log;

fn main() {
    init_log(true);
    let args = Args::parse();
    let recv_data = Arc::new(Mutex::new(RecvData::new()));
    let recv_data_final = Arc::clone(&recv_data);
    
    let (tx, _rx) = mpsc::channel::<Vec<u8>>();
    recv_data.lock().unwrap().tx = Some(tx);

    // Extract duration from args
    let port = args.port;
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
    let recv_data = recv_data_final.lock().unwrap();
    let non_received = recv_data.last_seq - recv_data.recevied;
    println!("Packet loss rate: {:.5}", non_received as f64 / recv_data.last_seq as f64);
    println!("Stuttering rate: {:.5}", recv_data.stutter.get_stuttering());

    // Write the data to stuttering file
    let mut logger = File::create( format!("logs/stuttering-{port}.txt", ) ).unwrap();
    for val in &recv_data.stutter.ack_times {
        logger.write_all(format!("{:?}\n", val).as_bytes()).unwrap();
    }
}

