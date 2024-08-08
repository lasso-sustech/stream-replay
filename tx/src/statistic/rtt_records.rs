use core::packet::PacketType;
use std::cmp::Ordering;
#[derive(Debug, Clone)]
struct RTTEntry {
    seq: usize,
    rtt: f64,
    channel_rtts: Vec<Option<f64>>,
    visited_rtt: Vec<bool>,
    completed: bool,
}

impl RTTEntry {
    fn new(seq: usize, max_links: usize) -> Self {
        RTTEntry {
            seq,
            rtt: 0.0,
            channel_rtts: vec![None; max_links],
            visited_rtt: vec![false; max_links + 1],
            completed: false,
        }
    }

    fn update_value(&mut self, channel: PacketType, value: f64)  {
        self.rtt = value;
        match channel {
            PacketType::SLFL => {
                self.channel_rtts[0] = Some(value);
                self.completed = true;
            },
            PacketType::SLSL => {
                self.channel_rtts[1] = Some(value);
                self.completed = true;
            },
            PacketType::DFL => {
                self.channel_rtts[0] = Some(value);
                self.completed = self.channel_rtts.iter().all(|rtt| rtt.is_some());
            } 
            PacketType::DSL => {
                self.channel_rtts[1] = Some(value);
                self.completed = self.channel_rtts.iter().all(|rtt| rtt.is_some());
            },
            _ => {panic!("Invalid packet type")}
        }
    }
}

pub struct RttRecords {
    queue: Vec<Option<RTTEntry>>,
    target_rtt: f64,
    max_length: usize,
    max_links: usize,
}

impl RttRecords {
    pub fn new(max_length: usize, max_links: usize, target_rtt: f64) -> Self {
        RttRecords {
            queue: vec![None; max_length],
            target_rtt,
            max_length,
            max_links,
        }
    }

    pub fn update(&mut self, seq: usize, channel: PacketType, rtt: f64) -> bool{
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
        self.queue[index].as_ref().unwrap().completed
    }

    fn average_between_quantiles(values: &mut Vec<f64>) -> f64 {
        if values.is_empty() {
            0.0
        } else {
            values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
            
            let len = values.len();
            let pos_10 = ((len as f64) * 0.10).floor() as usize;
            let pos_90 = ((len as f64) * 0.90).floor() as usize;
    
            let slice = &values[pos_10..pos_90];
            slice.iter().sum::<f64>() / slice.len() as f64
        }
    }

    pub fn statistic(&mut self) -> (f64, Vec<f64>, f64, Vec<f64>) {
        // Vectors to store RTT and channel RTT values
        let mut rtt_values = Vec::new();
        let mut channel_rtts = vec![Vec::new(); self.max_links];
    
        let mut outages = 0.0;
        let mut ch_outages = vec![0; self.max_links];
        let mut count = vec![0; self.max_links + 1];
    
        for entry in &mut self.queue {
            if let Some(ref mut entry) = entry {
                for (i, rtt_opt) in entry.channel_rtts.iter().enumerate() {
                    if let Some(rtt) = rtt_opt {
                        if !entry.visited_rtt[i + 1] {
                            entry.visited_rtt[i + 1] = true;
                            channel_rtts[i].push(*rtt);
                            if rtt > &self.target_rtt {
                                ch_outages[i] += 1;
                            }
                            count[i + 1] += 1;
                        }
                    }
                }
                if entry.completed && !entry.visited_rtt[0] {
                    rtt_values.push(entry.rtt);
                    if entry.rtt > self.target_rtt {
                        outages += 1.0;
                    }
                    count[0] += 1;
                    entry.visited_rtt[0] = true;
                }
            }
        }
    
        let rtt_avg = RttRecords::average_between_quantiles(&mut rtt_values);
    
        let outage_rate = if count[0] == 0 {
            0.0
        } else {
            outages / count[0] as f64
        };
    
        let ch_outage_rates: Vec<f64> = ch_outages
            .iter()
            .enumerate()
            .map(|(i, &x)| if count[i + 1] == 0 { 0.0 } else { x as f64 / count[i + 1] as f64 })
            .collect();
    
        let channel_rtts_avg: Vec<f64> = channel_rtts
            .iter_mut()
            .map(|rtts| RttRecords::average_between_quantiles(rtts))
            .collect();

        (rtt_avg, channel_rtts_avg, outage_rate, ch_outage_rates)
    }
    
}
