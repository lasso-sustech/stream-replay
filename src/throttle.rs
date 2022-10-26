use std::time::SystemTime;
use std::collections::VecDeque;

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

pub struct RateThrottle {
    window: SlidingWindow<(TIME, SIZE)>,
    pub throttle: f64,
}

impl RateThrottle {
    pub fn new(throttle: f64, window_size:usize) -> Self {
        let window = SlidingWindow::new(window_size);
        Self{ window, throttle }
    }

    pub fn exceeds_with(&mut self, size_bytes:usize) -> bool {
        if self.throttle==0.0 || self.window.window.len()==0 {
            self.window.push(( SystemTime::now(), size_bytes ));
            return false;
        }

        let acc_size: usize = self.window.window.iter().map(|&x| x.1).sum();
        let acc_size = acc_size  + size_bytes;

        let acc_time = SystemTime::now().duration_since( self.window.window.get(0).unwrap().0 ).unwrap();
        let acc_time = acc_time.as_nanos(); // + interval_ns as u128;

        let average_rate_mbps = 8.0 * (acc_size as f64/1e6) / (acc_time as f64*1e-9);
        if average_rate_mbps < self.throttle {
            self.window.push(( SystemTime::now(), size_bytes ));
            false
        }
        else {
            true
        }
    }
}
