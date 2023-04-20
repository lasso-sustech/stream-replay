use std::io::prelude::*;
use std::sync::{Arc, Mutex};
use std::thread::{self,};
use std::time::Duration;

type GuardedRate = Arc<Mutex<f64>>;
type RateRecords = Vec<GuardedRate>;
type GuardedRateRecords = Arc<Mutex<RateRecords>>;
pub struct RateWatch {
    vec_rate: GuardedRateRecords
}

fn rate_watch_thread(vec_rate: GuardedRateRecords) {
    loop {
        thread::sleep( Duration::from_millis(200) );
        if let Ok(vec_rate) = vec_rate.try_lock() {
            let _results:Vec<_> = vec_rate.iter().enumerate().map(|(i,rate)| {
                let _rate = rate.lock().unwrap();
                format!("({}) {:.3} Mbps", i+1, _rate)
            }).collect();
            print!("\r{:?}", _results);
            std::io::stdout().flush().unwrap_or(());
        }
    }
}

impl RateWatch {
    pub fn new() -> Self {
        let vec_rate = Arc::new(Mutex::new( Vec::new() ));
        Self{ vec_rate }
    }

    pub fn register(&mut self, ref_rate:&GuardedRate) -> Option<()> {
        let mut _vec_rate = self.vec_rate.lock().ok()?;
        Some( _vec_rate.push( Arc::clone(ref_rate) ) )
    }

    pub fn start(&self) {
        let _vec_rate = self.vec_rate.clone();
        thread::spawn(move|| {rate_watch_thread(_vec_rate)});
    }
}
