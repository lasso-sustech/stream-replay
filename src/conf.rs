use rand::prelude::*;
use std::path::Path;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug,Clone)]
pub struct ConnParams {
    pub npy_file: String,
    pub port: Option<u16>,
    pub tos: Option<u8>,
    pub throttle: Option<f64>,
    pub priority: Option<String>
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

        write!(f, "{} {{ ", _type)?;
        {
            if let Some(port) = _param.port {
                write!(f, "port: {}, ", port)?;
            }
            if let Some(tos) = _param.tos {
                write!(f, "tos: {}, ", tos)?;
            }
            if let Some(throttle) = _param.throttle {
                write!(f, "throttle: {} Mbps, ", throttle)?;
            }
        }
        write!(f, "file:\"{}\"}}", _file)
    }
}

impl StreamParam {
    pub fn validate(mut self, root:Option<&Path>) -> Option<Self> {
        let mut rng = rand::thread_rng();
        let ( Self::TCP(ref mut param) | Self::UDP(ref mut param) ) = self;

        // validate npy file existing
        let cwd = std::env::current_dir().unwrap();
        let path_trail1 = cwd.join( &param.npy_file );
        let path_trail2 = root.unwrap_or( cwd.as_path() ).join( &param.npy_file );

        if path_trail1.exists() {
            param.npy_file = String::from( path_trail1.to_str().unwrap() );
        }
        else if path_trail2.exists() {
            param.npy_file = String::from( path_trail2.to_str().unwrap() );
        }
        else {
            return None;
        }

        // validate port number, or random
        if ! (1024..).contains( &param.port.unwrap_or(0) ) {
            param.port = Some( rng.gen_range(1025..=65535) );
        }

        Some(self)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Manifest {
    pub window_size: usize,
    pub streams: Vec<StreamParam>
}
