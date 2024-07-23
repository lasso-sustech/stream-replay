use std::{net::UdpSocket, collections::HashMap, time::{Duration, SystemTime}};
use serde::{Serialize, Deserialize};
use crate::source::SourceManager;

#[derive(Serialize, Deserialize, Debug,Clone)]
pub struct Statistics {
    pub rtt: Option<f64>,
    pub channel_rtts: Option<Vec<f64>>,
    pub outage_rate : Option<f64>,
    pub ch_outage_rates: Option<Vec<f64>>,
    pub throughput: f64,
    pub tx_parts: Vec<f64>,
}

#[derive(Serialize, Deserialize, Debug,Clone)]
enum RequestValue {
    Throttle(HashMap<String, f64>),
    TxPart(HashMap<String, Vec<f64>>),
    Statistics(HashMap<String, f64>),
}

#[derive(Serialize, Deserialize, Debug,Clone)]
enum ResponseValue {
    Statistics(HashMap<String,Statistics>),
}

#[derive(Serialize, Deserialize, Debug,Clone)]
struct Request {
    cmd: RequestValue,
}

#[derive(Serialize, Deserialize, Debug,Clone)]
struct Response {
    cmd: ResponseValue,
}

pub struct IPCDaemon {
    ipc_port: u16,
    tx_ipaddr: String,
    sources: HashMap<String, SourceManager>
}

impl IPCDaemon {
    pub fn new(sources: HashMap<String, SourceManager>, ipc_port: u16, tx_ipaddr:String) -> Self {
        Self{ sources, ipc_port, tx_ipaddr }
    }

    fn handle_request(&self, req:Request) -> Option<Response> {
        match req.cmd {
            RequestValue::Throttle(data) => {
                let _:Vec<_> = data.iter().map(|(name, value)| {
                    self.sources[name].throttle(*value);
                }).collect();
                
                // TODO: reset RTT records for all
                //
                return None;
            },

            RequestValue::TxPart(data) => {
                let _:Vec<_> = data.iter().map(|(name, value)| {
                    self.sources[name].set_tx_parts(value.clone());
                }).collect();
                //
                return None;
            },

            RequestValue::Statistics(_)  => {
                let body = Some( self.sources.iter().filter_map(|(name,src)| {
                    match src.statistics() {
                        Some(stat) => Some(( name.clone(), stat )),
                        None => None
                    }
                }).collect() );
                //
                return Some(Response{ cmd: ResponseValue::Statistics(body.unwrap())});
            }
        }
    }

    pub fn start_loop(&self, duration:f64) {
        let deadline = SystemTime::now() + Duration::from_secs_f64(duration);
        let addr = format!("{}:{}",self.tx_ipaddr, self.ipc_port);
        let sock = UdpSocket::bind(&addr).unwrap();
        sock.set_nonblocking(true).unwrap();
        let mut buf = [0; 2048];

        while SystemTime::now() < deadline {
            if let Ok((len, src_addr)) = sock.recv_from(&mut buf) {
                let buf_str = std::str::from_utf8(&buf[..len]).unwrap();
                let req = serde_json::from_str::<Request>(buf_str).unwrap();
                if let Some(res) = self.handle_request(req) {
                    let res = serde_json::to_string(&res).unwrap();
                    sock.send_to(res.as_bytes(), src_addr).unwrap();
                }
            }
            std::thread::sleep( Duration::from_nanos(10_000_000) );
        }
    }
}
