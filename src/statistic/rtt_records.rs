use crate::packet::PacketType;

#[derive(Debug, Clone)]
struct RTTEntry {
    seq: usize,
    rtt: f64,
    channel_rtts: Vec<Option<f64>>,
    visited: bool,
    completed: bool,
}

impl RTTEntry {
    fn new(seq: usize, max_links: usize) -> Self {
        RTTEntry {
            seq,
            rtt: 0.0,
            channel_rtts: vec![None; max_links],
            visited: false,
            completed: false,
        }
    }

    fn update_value(&mut self, channel: PacketType, value: f64)  {
        self.rtt = value;
        match channel {
            PacketType::SL => {
                self.channel_rtts[0] = Some(value);
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

    pub fn statistic(&mut self) -> (f64, Vec<f64>) {
        //get the average rtt and list of average channel rtt
        let mut rtt_sum = 0.0;
        let mut channel_rtts = vec![0.0; self.max_links];
        let mut count = vec![0; self.max_links + 1];
        for entry in &mut self.queue {
            if let Some(ref mut entry) = entry {
                if !entry.visited && entry.completed{
                    entry.visited = true;
                    count[0] += 1;
                    rtt_sum += entry.rtt;
                    for (i, rtt_opt) in entry.channel_rtts.iter().enumerate() {
                        if let Some(rtt) = rtt_opt {
                            channel_rtts[i] += rtt;
                            count[i + 1] += 1;
                        }
                    }
                }
            }
        }
        let rtt_avg = if count[0] == 0 {
            0.0
        } else {
            rtt_sum / count[0] as f64
        };
        
        let channel_rtts_avg: Vec<f64> = channel_rtts
            .iter()
            .enumerate()
            .map(|(i, &x)| if count[i + 1] == 0 { 0.0 } else { x / count[i + 1] as f64 })
            .collect();
        (rtt_avg, channel_rtts_avg)
    }
}
