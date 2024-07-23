use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};
use ndarray::prelude::*;
use ndarray_npy::read_npy;
use log::trace;

use rand::seq::SliceRandom;
use rand::thread_rng;

use crate::conf::{StreamParam, ConnParams};
use crate::packet::*;
use crate::dispatcher::dispatch;
use crate::throttle::RateThrottler;
use crate::rtt::{RttRecorder,RttSender};
use crate::ipc::Statistics;
use crate::tx_part_ctl::TxPartCtler;

type GuardedThrottler = Arc<Mutex<RateThrottler>>;
type GuardedTxPartCtler = Arc<Mutex<TxPartCtler>>;

pub fn source_thread(throttler:GuardedThrottler, tx_part_ctler:GuardedTxPartCtler, rtt_tx: Option<RttSender>,
    params: ConnParams, socket_infos:HashMap<String, Sender<PacketStruct>>)
{
    let trace: Array2<u64> = read_npy(&params.npy_file).expect("loading failed.");
    let (start_offset, duration) = (params.start_offset, params.duration);
    let mut template = PacketStruct::new(params.port);
    let spin_sleeper = spin_sleep::SpinSleeper::new(100_000)
                        .with_spin_strategy(spin_sleep::SpinStrategy::YieldThread);

    let mut loops = 0;
    let mut idx = start_offset;
    let stop_time  = SystemTime::now().checked_add( Duration::from_secs_f64(duration[1]) ).unwrap();

    spin_sleeper.sleep( Duration::from_secs_f64(duration[0]) );
    while SystemTime::now() <= stop_time {
        loops += 1;

        let deadline = if loops < params.loops {
            // 0. next iteration
            idx = (idx + 1) % trace.shape()[0];
            let size_bytes = trace[[idx, 1]] as usize;
            let interval_ns = trace[[idx, 0]];

            // 1. generate packets
            let mut packets = Vec::new();
            let (_num, _remains) = (size_bytes/MAX_PAYLOAD_LEN, size_bytes%MAX_PAYLOAD_LEN); 
            let num = _num + if _remains > 0 { 1 } else { 0 };
            template.next_seq(_num, _remains);
            let mut packet_states: Vec<Vec<(u16, PacketType)>> = tx_part_ctler.lock().unwrap().get_packet_states(num);

            let mut rng = thread_rng();
            packet_states.shuffle(&mut rng);

            for packet_state in packet_states {
                for (offset, packet_type) in packet_state {
                    let length = if offset == (num - 1) as u16 {
                        _remains as u16
                    } else {
                        MAX_PAYLOAD_LEN as u16
                    };
                    
                    template.set_length(length);
                    template.set_offset(offset);
                    template.set_indicator(packet_type);
                    
                    packets.push(template.clone());
                }
            }
            // 2. append to application-layer queue
            throttler.lock().unwrap().prepare( packets );
            // report RTT
            if let Some(ref r_tx) = rtt_tx {
                r_tx.send(template.seq).unwrap();
            }

            SystemTime::now() + Duration::from_nanos(interval_ns)
        }
        else {
            stop_time
        };
        
        
        trace!("Source: Time {} -> seq {}", SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64(), template.seq as u32);
        // 3. process queue, aware of blocked status
        while SystemTime::now() < deadline {
            match throttler.lock().unwrap().try_consume(|mut packet| {
                let time_now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64();
                packet.timestamp = time_now;
                match tx_part_ctler.lock() {
                    Ok(controller) => {
                        let ip_addr = controller.packet_to_ipaddr(packet.indicators.clone());
                        if let Some(sender) = socket_infos.get(&ip_addr) {
                            sender.send(packet).unwrap();
                        }
                    }
                    Err(_) => (),
                }
                true
            }) {
                Some(_) => continue,
                None=> break
            }
        }

        // 4. sleep until next arrival
        if let Ok(remaining_time) = deadline.duration_since( SystemTime::now() ) {
            spin_sleeper.sleep( remaining_time );
        }
    }

    //reset throttler
    throttler.lock().unwrap().reset();
}

pub struct SourceManager{
    pub name: String,
    stream: StreamParam,
    //
    start_timestamp: SystemTime,
    stop_timestamp: SystemTime,
    //
    throttler: GuardedThrottler,
    rtt: Option<RttRecorder>,
    tx_part_ctler: Arc<Mutex<TxPartCtler>>,
    //
    socket_infos: Vec<HashMap<String, Sender<PacketStruct>>>,
}

impl SourceManager {
    pub fn new(stream: StreamParam, window_size:usize) -> Self {
        let (StreamParam::UDP(ref params) | StreamParam::TCP(ref params)) = stream;
        let name = stream.name();

        let socket_infos = vec![dispatch(params.links.clone(), params.tos)].into();
        
        
        let throttler = Arc::new(Mutex::new(
            RateThrottler::new(name.clone(), params.throttle, window_size, params.no_logging, params.loops != usize::MAX)
        ));
        let link_num = params.links.len();
        let target_rtt = params.target_rtt;
        let rtt =  match params.calc_rtt {
            false => None,
            true => Some( RttRecorder::new( &name, params.port, link_num, target_rtt) )
        };

        let tx_part_ctler = Arc::new(Mutex::new(
            TxPartCtler::new(params.tx_parts.clone(), params.links.clone())
        ));

        let start_timestamp = SystemTime::now();
        let stop_timestamp = SystemTime::now();

        Self{ name, stream, throttler, rtt, tx_part_ctler, socket_infos, start_timestamp, stop_timestamp }
    }

    pub fn throttle(&self, throttle:f64) {
        if let Ok(ref mut throttler) = self.throttler.lock() {
            throttler.throttle = throttle;
        }
    }

    pub fn set_tx_parts(&self, tx_parts:Vec<f64>) {
        if let Ok(ref mut tx_part_ctler) = self.tx_part_ctler.lock() {
            if tx_parts.len() != tx_part_ctler.tx_parts.len() {
                return;
            }
            tx_part_ctler.set_tx_parts(tx_parts);
        }
    }

    pub fn statistics(&self) -> Option<Statistics> {
        let now = SystemTime::now();
        if now < self.start_timestamp || now > self.stop_timestamp {
            return None;
        }
    
        let throughput = self.throttler.lock().ok()?.last_rate;
    
        let (rtt, channel_rtts, outage_rate, ch_outage_rates) = if let Some(ref rtt) = self.rtt {
            let stats = rtt.rtt_records.lock().unwrap().statistic();
            (Some(stats.0), Some(stats.1), Some(stats.2), Some(stats.3))
        } else {
            (None, None, None, None)
        };
    
        let tx_parts = self.tx_part_ctler.lock().ok()?.tx_parts.clone();
    
        Some(Statistics { rtt, channel_rtts, outage_rate, ch_outage_rates, throughput, tx_parts })
    }

    pub fn start(&mut self, index:usize, tx_ipaddr:String) -> JoinHandle<()> {
        let throttler = Arc::clone(&self.throttler);
        let tx_part_ctler = Arc::clone(&self.tx_part_ctler);
        let rtt_tx = match self.rtt {
            Some(ref mut rtt) => Some( rtt.start(tx_ipaddr) ),
            None => None
        };
        let (StreamParam::UDP(ref params) | StreamParam::TCP(ref params)) = self.stream;
        let params = params.clone();

        let _now = SystemTime::now();
        self.start_timestamp = _now + Duration::from_secs_f64( params.duration[0] );
        self.stop_timestamp = _now + Duration::from_secs_f64( params.duration[1] );

        let socket_infos = self.socket_infos.pop().unwrap();
        let source = thread::spawn(move || {
            source_thread(throttler, tx_part_ctler, rtt_tx, params, socket_infos);
        });

        println!("{}. {} on ...", index, self.stream);
        source
    }
}
