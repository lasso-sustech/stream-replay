pub struct Stutter {
    pub start_time: f64,
    pub end_time: f64,
    pub ack_times: Vec<f64>,
}

impl Stutter {
    pub fn new() -> Self {
        Stutter {
            start_time: 0.0,
            end_time: 0.0,
            ack_times: vec![],
        }
    }

    pub fn update(&mut self, time: f64) {
        if self.start_time == 0.0 {
            self.start_time = time;
        } else {
            self.end_time = time;
        }
        self.ack_times.push(time);
    }

    pub fn get_stuttering(&self) -> f64 {
        if self.ack_times.len() < 2 {
            return 0.0;
        }
        let mut stuttering = 0.0;
        for i in 1..self.ack_times.len() {
            let diff = self.ack_times[i] - self.ack_times[i - 1] - 0.016;
            if diff > 0.016 {
                stuttering += diff;
            }
        }
        if stuttering == 0.0 {
            return 0.0;
        }


        stuttering / (self.end_time - self.start_time)
    }

}