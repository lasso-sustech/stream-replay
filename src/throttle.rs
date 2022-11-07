use std::time::SystemTime;
use std::fs::File;
use std::io::prelude::*;
use std::collections::VecDeque;
use crate::PacketStruct;

type TIME = SystemTime;
type SIZE = usize;

struct SlidingWindow<T> {
    size: usize,
    pub window: VecDeque<T>
}

impl<T> SlidingWindow<T>
where T:Sized + Copy
{
    pub fn new(size: usize) -> Self {
        let window = VecDeque::with_capacity(size);
        Self{ size, window }
    }

    pub fn push(&mut self, item: T) {
        if self.window.len()==self.size {
            self.window.pop_front();
        }
        self.window.push_back(item);
    }
}

pub struct RateThrottler {
    logger: File,
    window: SlidingWindow<(TIME, SIZE)>,
    buffer: VecDeque<PacketStruct>,
    pub throttle: f64,
}

impl RateThrottler {
    pub fn new(name:String, throttle: f64, window_size:usize) -> Self {
        let buffer = VecDeque::new();
        let logger = File::create( format!("data/log-{}.txt", name) ).unwrap();
        let window = SlidingWindow::new(window_size);
        Self{ logger, window, buffer, throttle }
    }

    pub fn current_rate_mbps(&self, extra_bytes:Option<usize>) -> f64 {
        let acc_size: usize = self.window.window.iter().map(|&x| x.1).sum();
        let acc_size = acc_size  + extra_bytes.unwrap_or(0);

        let acc_time = SystemTime::now().duration_since( self.window.window.get(0).unwrap().0 ).unwrap();
        let acc_time = acc_time.as_nanos();

        let average_rate_mbps = 8.0 * (acc_size as f64/1e6) / (acc_time as f64*1e-9);
        average_rate_mbps
    }

    pub fn prepare(&mut self, packets: Vec<PacketStruct>) {
        let timestamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64();
        self.logger.write_all( format!("{:.9} {}\n", timestamp, self.buffer.len()).as_bytes() ).unwrap();
        for packet in packets.into_iter() {
            self.buffer.push_back(packet);
        }
        // self.buffer.push_back(value)
    }

    pub fn view(&self) -> Option<&PacketStruct> {
        self.buffer.front()
    }

    // pub fn try_consume(&mut self, f:T)
    // where T: Fn(&PacketStruct) -> Result<(), std::io::Error> {

    // }

    pub fn consume(&mut self) -> Option<PacketStruct> {
        let timestamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64();
        self.logger.write_all( format!("{:.9} {}\n", timestamp, self.buffer.len()).as_bytes() ).unwrap();
        self.buffer.pop_front()
    }

    pub fn exceeds_with(&mut self, size_bytes:usize) -> bool {
        if self.throttle==0.0 || self.window.window.len()==0 {
            self.window.push(( SystemTime::now(), size_bytes ));
            return false;
        }

        let average_rate_mbps = self.current_rate_mbps( Some(size_bytes) );
        if average_rate_mbps < self.throttle {
            self.window.push(( SystemTime::now(), size_bytes ));
            false
        }
        else {
            true
        }
    }
}
