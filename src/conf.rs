use std::path::Path;
use rand::prelude::*;
use rand::distributions::Standard;
use serde::{Serialize, Deserialize};

const fn _default_duration() -> [f64; 2] { [0.0, f64::MAX] }
const fn _default_loops() -> usize { usize::MAX }
fn _random_value<T>() -> T where Standard: Distribution<T> { rand::thread_rng().gen() }
#[derive(Serialize, Deserialize, Debug,Clone)]
pub struct ConnParams {
    pub npy_file: String,
    #[serde(default = "_random_value")]     //default:
    pub port: u16,                          //         <random>
    #[serde(default = "_default_duration")] //default:
    pub duration: [f64; 2],                 //         [0.0, +inf]
    #[serde(default = "_random_value")]     //default:
    pub start_offset: usize,                //         <random>
    #[serde(default = "_default_loops")]    //default:
    pub loops: usize,                       //         +inf
    #[serde(default)] pub tos: u8,          //default: 0
    #[serde(default)] pub throttle: f64,    //default: 0.0
    #[serde(default)] pub priority: String, //default: ""
    #[serde(default)] pub calc_rtt: bool,   //default: false
    #[serde(default)] pub no_logging: bool, //default: false
    #[serde(default)] pub tx_ipaddrs: Vec<String>, //default: []
    #[serde(default)] pub tx_parts: Vec<f64>, //default: []
}


#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum StreamParam {
    TCP(ConnParams),
    UDP(ConnParams)
}

impl std::fmt::Display for StreamParam {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let (_type, _param) =
        match self {
            Self::TCP(p) => ("TCP", p),
            Self::UDP(p) => ("UDP", p)
        };
        let _file:String = _param.npy_file.clone();

        write!(f,
            "{type} {{ port: {port}, tos: {tos}, throttle: {throttle} Mbps, file: \"{file}\", loops: {loops} }}",
            type=_type, port=_param.port, tos=_param.tos, throttle=_param.throttle, loops=_param.loops as isize, file=_file
        )
    }
}

impl StreamParam {
    pub fn validate(mut self, _root:Option<&Path>, duration:f64) -> Option<Self> {
        let ( Self::TCP(ref mut param) | Self::UDP(ref mut param) ) = self;

        // validate duration
        if param.duration[1] > duration {
            param.duration[1] = duration;
        }

        // TODO: valid ip address

        Some(self)
    }

    pub fn name(&self) -> String {
        match self {
            StreamParam::TCP(params) => format!("{}@{}", params.port, params.tos),
            StreamParam::UDP(params) => format!("{}@{}", params.port, params.tos),
        }
    }

}

#[derive(Serialize, Deserialize, Debug)]
pub struct Manifest {
    pub use_agg_socket: Option<bool>,
    pub orchestrator: Option<String>,
    pub window_size: usize,
    pub streams: Vec<StreamParam>,
    pub tx_ipaddrs: Vec<String>
}
