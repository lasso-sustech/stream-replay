use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};
use ndarray::prelude::*;
use ndarray_npy::{read_npy,ReadNpyExt};
use std::sync::mpsc::channel;

use crate::conf::{StreamParam, ConnParams};
use crate::packet::*;
use crate::broker::GlobalBroker;
use crate::throttle::RateThrottler;
use crate::rtt::{RttRecorder,RttSender};
use crate::dispatcher::BlockedSignal;
use crate::ipc::Statistics;
use crate::tx_part_ctl::TxPartCtler;

type GuardedThrottler = Arc<Mutex<RateThrottler>>;
type GuardedTxPartCtler = Arc<Mutex<TxPartCtler>>;

pub fn stream_thread(throttler:GuardedThrottler, tx_part_ctler:GuardedTxPartCtler, rtt_tx: Option<RttSender>,
    params: ConnParams, tx:PacketSender, blocked_signal:BlockedSignal, dest: BufferReceiver)
{
    let mut template = PacketStruct::new(params.port);
    let stop_time  = SystemTime::now().checked_add( Duration::from_secs_f64(params.duration[1]) ).unwrap();

    while SystemTime::now() <= stop_time {
        // 0. wait for the next packet
        let buffer = dest.recv().unwrap();
        let size_bytes = buffer.len();

        // 1. generate packets
        let mut packets = Vec::new();
        let (_num, _remains) = (size_bytes/MAX_PAYLOAD_LEN, size_bytes%MAX_PAYLOAD_LEN);
        let num = _num + if _remains > 0 { 1 } else { 0 };
        template.next_seq(_num, _remains);
        template.set_length(MAX_PAYLOAD_LEN as u16);
        let mut packet_states = tx_part_ctler.lock().unwrap().get_packet_states(num);
        for idx in 0..num {
            template.next_offset();
            if idx == num-1 {
                template.set_length(_remains as u16);
                template.set_payload(&buffer[(idx*MAX_PAYLOAD_LEN)..]);
            } else {
                template.set_payload(&buffer[(idx*MAX_PAYLOAD_LEN)..((idx+1)*MAX_PAYLOAD_LEN)]);
            }
            let packet_types = packet_states.remove(0);
            for packet_type in packet_types {
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

        // 3. process queue, aware of blocked status
        while SystemTime::now() < stop_time {
            let _signal = blocked_signal.lock().unwrap();
            if !(*_signal) {
                match throttler.lock().unwrap().try_consume(|mut packet| {
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
    }

    //reset throttler
    throttler.lock().unwrap().reset();
}

pub fn source_thread(throttler:GuardedThrottler, tx_part_ctler:GuardedTxPartCtler, rtt_tx: Option<RttSender>,
    params: ConnParams, tx:PacketSender, blocked_signal:BlockedSignal)
{
    let trace: Array2<u64> = if cfg!(target_os="android") {
        let asset_path = std::ffi::CString::new(params.npy_file).unwrap();
        let asset_reader = unsafe {
            if let Some(am) = crate::android::ASSET_MANAGER.as_ref() {
                am.open(&asset_path).unwrap()
            } else {
                panic!("Asset Manager is not initialized.")
            }
        };
        Array2::<u64>::read_npy(asset_reader).unwrap()
    } else {
        read_npy(&params.npy_file).expect("loading failed.")
    };
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
            template.set_length(MAX_PAYLOAD_LEN as u16);
            let mut packet_states = tx_part_ctler.lock().unwrap().get_packet_states(num);
            for idx in 0..num {
                template.next_offset();
                if idx == num-1 {
                    template.set_length(_remains as u16);
                }
                let packet_types = packet_states.remove(0);
                for packet_type in packet_types {
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
        
        // 3. process queue, aware of blocked status
        while SystemTime::now() < deadline {
            let _signal = blocked_signal.lock().unwrap();
            if !(*_signal) {
                match throttler.lock().unwrap().try_consume(|mut packet| {
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
    pub source: Vec<BufferSender>,
    dest: Vec<BufferReceiver>,
    //
    start_timestamp: SystemTime,
    stop_timestamp: SystemTime,
    //
    throttler: GuardedThrottler,
    rtt: Option<RttRecorder>,
    tx_part_ctler: Arc<Mutex<TxPartCtler>>,
    //
    tx: Vec<PacketSender>,
    blocked_signal: BlockedSignal
}

impl SourceManager {
    pub fn new(stream: StreamParam, window_size:usize, broker:&mut GlobalBroker) -> Self {
        let (StreamParam::UDP(ref params) | StreamParam::TCP(ref params)) = stream;
        let name = stream.name();
        let (tx, blocked_signal, tx_part_ctler) = broker.add(params.clone());
        let tx = [tx].into();
        
        let throttler = Arc::new(Mutex::new(
            RateThrottler::new(name.clone(), params.throttle, window_size, params.no_logging, params.loops != usize::MAX)
        ));
        let link_num = params.links.len() as u16;
        let rtt =  match params.calc_rtt {
            false => None,
            true => Some( RttRecorder::new( &name, params.port, link_num ) )
        };

        let start_timestamp = SystemTime::now();
        let stop_timestamp = SystemTime::now();

        let (source, dest) = if params.npy_file.starts_with(STREAM_PROTO) {
            let (tx, rx) = channel();
            (vec![tx], vec![rx])
        } else {
            (vec![], vec![])
        };

        Self{ name, stream, throttler, rtt, tx_part_ctler, tx, blocked_signal, start_timestamp, stop_timestamp, source, dest }
    }

    pub fn throttle(&self, throttle:f64) {
        if let Ok(ref mut throttler) = self.throttler.lock() {
            throttler.throttle = throttle;
        }
    }

    pub fn set_tx_parts(&self, tx_parts:Vec<f64>) {
        if let Ok(ref mut tx_part_ctler) = self.tx_part_ctler.lock() {
            tx_part_ctler.set_tx_parts(tx_parts);
        }
    }

    pub fn reset_rtt_records(&self) {
        if let Some(ref rtt) = self.rtt {
            if let Ok(mut records) = rtt.rtt_records.lock() {
                records.iter_mut().for_each(|(count,val)|
                {
                    *count = 0;
                    *val = 0.0;
                }
            ) 
            }
        }
    }

    pub fn statistics(&self) -> Option<Statistics> {
        let _now = SystemTime::now();
        if _now<self.start_timestamp || _now>self.stop_timestamp {
            return None;
        }

        let throughput = {
            match self.throttler.lock() {
                Err(_) => return None,
                Ok(throttler) => throttler.last_rate
            }
        };

        let rtt = {
            match &self.rtt {
                None => vec![0.0],
                Some(recorder) => match recorder.rtt_records.lock() {
                    Err(_) => return None,
                    Ok(vals) => {
                        let mut rtt = Vec::new();
                        for (count,val) in vals.iter() {
                            if *count > 0 {
                                rtt.push( *val/(*count as f64) );
                            }
                        }
                        rtt
                    }
                }
            }
        };
        
        Some( Statistics{rtt,throughput} )
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
        let tx = self.tx.pop().unwrap();
        let blocked_signal = Arc::clone(&self.blocked_signal);

        let _now = SystemTime::now();
        self.start_timestamp = _now + Duration::from_secs_f64( params.duration[0] );
        self.stop_timestamp = _now + Duration::from_secs_f64( params.duration[1] );

        let dest = self.dest.pop();
        let source = thread::spawn(move || {
            if params.npy_file.starts_with(STREAM_PROTO) {
                let dest = dest.unwrap();
                stream_thread(throttler, tx_part_ctler, rtt_tx, params, tx, blocked_signal, dest)
            } else {
                source_thread(throttler, tx_part_ctler, rtt_tx, params, tx, blocked_signal)
            }
        });

        println!("{}. {} on ...", index, self.stream);
        source
    }
}
