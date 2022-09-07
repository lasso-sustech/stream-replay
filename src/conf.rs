use rand::prelude::*;
use std::path::Path;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct ConnParams {
    npy_file: String,
    port: Option<u16>,
    tos: Option<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum StreamParam {
    TCP(ConnParams),
    UDP(ConnParams)
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
            param.port = rng.gen();
        }

        // validate tos value, or 0
        param.tos = Some( param.tos.unwrap_or(0) );

        Some(self)
    }
}

