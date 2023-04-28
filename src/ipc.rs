use std::{net::UdpSocket, collections::HashMap};
use serde::{Serialize, Deserialize};
use crate::source::SourceManager;

#[derive(Serialize, Deserialize, Debug,Clone)]
pub struct Statistics {
    pub rtt: Option<f64>,
    pub throughput: Option<f64>,
}

#[derive(Serialize, Deserialize, Debug,Clone)]
struct Request {
    cmd: String,
    body: Option<HashMap<String,f64>>
}

#[derive(Serialize, Deserialize, Debug,Clone)]
struct Response {
    cmd: String,
    body: Option<HashMap<String,Statistics>>
}

pub struct IPCDaemon {
    ipc_port: u16,
    sources:HashMap<String, SourceManager>
}

impl IPCDaemon {
    pub fn new(ipc_port: u16, sources:HashMap<String, SourceManager>) -> Self {
        Self{ ipc_port, sources }
    }

    pub fn start(&self) {
        let addr = format!("127.0.0.1:{}", self.ipc_port);
        let sock = UdpSocket::bind(&addr).unwrap();
        let mut buf = [0; 2048];

        while let Ok((_len, src_addr)) = sock.recv_from(&mut buf) {
            let buf_str = std::str::from_utf8(&buf).unwrap();
            let req = serde_json::from_str::<Request>(buf_str).unwrap();
            
            let res = {
                let body = match req.cmd.as_str() {
                    "throttle" => {
                        if let Some(data) = req.body {
                            let _:Vec<_> = data.iter().map(|(name, value)| {
                                self.sources[name].throttle(*value);
                            }).collect();
                        }
                        // reset RTT records for all
                        let _:Vec<_> = self.sources.iter().map(|(_,src)| {
                            src.reset_rtt_records()
                        }).collect();
                        None
                    },
                    "statistics" => {
                        Some( self.sources.iter().map(|(name, src)| (name.clone(), src.statistics())).collect() )
                    },
                    "stop" => {
                        break
                    }
                    _ => None
                };
                Response{ cmd:req.cmd.clone(), body }
            };

            let res = serde_json::to_string(&res).unwrap();
            sock.send_to(res.as_bytes(), src_addr).unwrap();
        }
    }
}
