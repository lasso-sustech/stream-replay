#[derive(Debug, Clone)]
struct RTTEntry {
    seq: usize,
    rtt: f64,
    channel_rtts: Vec<Option<f64>>,
}

impl RTTEntry {
    fn new(seq: usize, max_links: usize) -> Self {
        RTTEntry {
            seq,
            rtt: 0.0,
            channel_rtts: vec![None; max_links],
        }
    }

    fn update_value(&mut self, channel: usize, value: f64) {
        self.channel_rtts[channel] = Some(value);
        self.rtt = value // Assume no packet loss
    }
}

pub struct RttRecords {
    queue: Vec<Option<RTTEntry>>,
    max_length: usize,
    max_links: usize,
}

impl RttRecords {
    pub fn new(max_length: usize, max_links: usize) -> Self {
        RttRecords {
            queue: vec![None; max_length],
            max_length,
            max_links,
        }
    }

    pub fn update(&mut self, seq: usize, channel: usize, rtt: f64) {
        let index = seq % self.max_length;
        // If the entry is already present and seq value is the same, update the value
        // Otherwise, create a new entry
        match &mut self.queue[index] {
            Some(entry) => {
                if entry.seq == seq {
                    entry.update_value(channel, rtt);
                } else {
                    self.queue[index] = Some(RTTEntry::new(seq, self.max_links));
                    self.queue[index].as_mut().unwrap().update_value(channel, rtt);
                }
            }
            None => {
                self.queue[index] = Some(RTTEntry::new(seq, self.max_links));
                self.queue[index].as_mut().unwrap().update_value(channel, rtt);
            }
        }
    }

    pub fn statistic(&self) -> (f64, Vec<f64>) {
        //get the average rtt and list of average channel rtt
        let mut rtt_sum = 0.0;
        let mut channel_rtts = vec![0.0; self.max_links];
        let mut count = 0;
        for entry in &self.queue {
            if let Some(entry) = entry {
                count += 1;
                rtt_sum += entry.rtt;
                for i in 0..self.max_links {
                    if let Some(rtt) = entry.channel_rtts[i] {
                        channel_rtts[i] += rtt;
                    }
                }
            }
        }
        let rtt_avg = if count == 0 {
            0.0
        } else {
            rtt_sum / count as f64
        };
        let channel_rtts_avg = channel_rtts.iter().map(|&x| x / count as f64).collect();
        (rtt_avg, channel_rtts_avg)
    }
}
