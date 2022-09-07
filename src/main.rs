mod conf;

use std::path::{Path};
use clap::{Parser, };
use serde_json;
use ndarray::Array2;
use ndarray_npy::read_npy;

use crate::conf::{StreamParam, };

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about=None)]
struct ProgArgs {
    /// The manifest file tied with the data trace.
    #[clap( value_parser )]
    manifest_file: String,
    // #[clap( value_parser=value_parser!(u16).range(1024..) )]
    // port: u16,
    // #[clap( long, action, default_value_t=false )]
    // use_tcp: bool,
    // #[clap( default_value_t=0, value_parser=value_parser!(u8) )]
    // tos: u8,
}

fn main() {
    // read the manifest file
    let args = ProgArgs::parse();
    let file = std::fs::File::open(&args.manifest_file).unwrap();
    let reader = std::io::BufReader::new( file );
    let root = Path::new(&args.manifest_file).parent();
    let streams: Vec<StreamParam> = serde_json::from_reader(reader).unwrap();
    let streams: Vec<_> = streams.into_iter().filter_map( |x| x.validate(root) ).collect();

    // read the file
    // let _arr: Array2<f32> = read_npy(args.manifest_file).unwrap();

    // 
    
    println!("Hello, world!");
}
