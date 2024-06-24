use std::os::raw::c_char;
use std::ffi::CStr;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use serde_json;

use crate::conf::Manifest;
use crate::conf::StreamParam;
use crate::ipc::IPCDaemon;
use crate::source::SourceManager;
use crate::broker::GlobalBroker;
use crate::packet::*;
use crate::rx::{recv_thread, RecvData};

use jni::JNIEnv;
use jni::objects::{JClass, JObject, JString, JByteArray};

use ndk::asset::AssetManager;

pub static mut ASSET_MANAGER: Option<AssetManager> = None;
pub static mut SENDER_MAP: Option<HashMap<String, PacketSender>> = None;

#[cfg(target_os="android")]
#[no_mangle]
pub extern "C" fn start_tx(
    manifest_file: *const c_char,
    ipaddr1_tx: *const c_char, ipaddr1_rx: *const c_char,
    ipaddr2_tx: *const c_char, ipaddr2_rx: *const c_char,
    duration: f64,
    ipc_port: u16,
)
{
    unsafe {
        if let None = SENDER_MAP {
            SENDER_MAP = Some(HashMap::new());
        }
    }

    let manifest_file: String = unsafe { CStr::from_ptr(manifest_file).to_string_lossy().into_owned() };
    let ipaddr1_tx: String = unsafe { CStr::from_ptr(ipaddr1_tx).to_string_lossy().into_owned() };
    let ipaddr2_tx: String = unsafe { CStr::from_ptr(ipaddr2_tx).to_string_lossy().into_owned() };
    let ipaddr1_rx: String = unsafe { CStr::from_ptr(ipaddr1_rx).to_string_lossy().into_owned() };
    let _ipaddr2_rx: String = unsafe { CStr::from_ptr(ipaddr2_rx).to_string_lossy().into_owned() };

    //load the manifest file
    let asset_path = std::ffi::CString::new(manifest_file).unwrap();
    let asset_reader = unsafe {
        if let Some(am) = crate::android::ASSET_MANAGER.as_ref() {
            am.open(&asset_path).unwrap()
        } else {
            panic!("Asset Manager is not initialized.")
        }
    };
    let mut manifest:Manifest = serde_json::from_reader(asset_reader).unwrap();
    //update manifest file
    manifest.tx_ipaddrs = vec![ipaddr1_tx.clone(), ipaddr2_tx.clone()];
    manifest.streams.iter_mut().for_each(|stream| {
        match stream {
            StreamParam::TCP(param) | StreamParam::UDP(param) => {
                param.tx_ipaddrs = vec![ipaddr1_tx.clone(), ipaddr2_tx.clone()];
            }
        }
    });

    // parse the manifest file
    let streams:Vec<_> = manifest.streams.into_iter().filter_map( |x| x.validate(None, duration) ).collect();
    let window_size = manifest.window_size;
    let orchestrator = manifest.orchestrator;
    println!("Sliding Window Size: {}.", window_size);
    println!("Orchestrator: {:?}.", orchestrator);

    // from manifest create a mapping from port to target addr
    let mut port2ip: HashMap<u16, Vec<String>> = HashMap::new();
    for stream in &streams {
        let (port, ip) = match stream {
            StreamParam::TCP(param) => (param.port.clone(), param.tx_ipaddrs.clone()),
            StreamParam::UDP(param) => (param.port.clone(), param.tx_ipaddrs.clone())
        };
        port2ip.insert(port, ip);
    }

    // start broker
    let mut broker = GlobalBroker::new( orchestrator, ipaddr1_rx, port2ip);
    let _handle = broker.start();

    // spawn the source thread
    let mut sources:HashMap<_,_> = streams.into_iter().map(|stream| {
        let src = SourceManager::new(stream, window_size, &mut broker);
        if !src.source.is_empty() {
            unsafe {
                SENDER_MAP.as_mut().unwrap().insert(src.name.clone(), src.source[0].clone());
            }
        }
        let name = src.name.clone();
        (name, src)
    }).collect();
    let _handles:Vec<_> = sources.iter_mut().enumerate().map(|(i,(_name,src))| {
        src.start(i+1, String::from("0.0.0.0"))
    }).collect();

    // start global IPC
    let ipc = IPCDaemon::new( sources, ipc_port, String::from("0.0.0.0"));
    ipc.start_loop( duration );

    std::process::exit(0); //force exit
}

#[cfg(target_os="android")]
#[no_mangle]
pub extern "C" fn start_rx(
    port: u16,
    duration: u32,
    calc_rtt: bool,
)
{
    let args = crate::rx::Args { port, duration, calc_rtt };
    let recv_data = Arc::new(Mutex::new(RecvData::new()));
    let recv_data_thread = Arc::clone(&recv_data);

    // Extract duration from args
    let duration = args.duration.clone();
    
    let lock = Arc::new(Mutex::new(false));
    let lock_clone = Arc::clone(&lock);
    
    std::thread::spawn(move || {
        recv_thread(args, recv_data_thread, lock_clone);
    });

    
    while !*lock.lock().unwrap() {
        std::thread::sleep(std::time::Duration::from_nanos(100_000) );
    }

    // Sleep for the duration
    std::thread::sleep(std::time::Duration::from_secs(duration as u64));
    let data_len = recv_data.lock().unwrap().data_len;
    println!("Received Bytes: {:.3} MB", data_len as f64/ 1024.0 / 1024.0);
    let rx_duration = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64() - recv_data.lock().unwrap().rx_start_time;
    println!("Average Throughput: {:.3} Mbps", data_len as f64 / rx_duration / 1e6 * 8.0);
}

#[cfg(target_os="android")]
#[allow(non_snake_case)]
#[allow(dead_code)]
pub extern "system" fn Java_com_github_magicsih_androidscreencaster_RustStreamReplay_start_tx(
    mut env: JNIEnv, _: JClass,
    asset_manager: JObject,
    manifest_file: JString,
    ipaddr1_tx: JString, ipaddr1_rx: JString,
    ipaddr2_tx: JString, ipaddr2_rx: JString,
    duration: f64,
    ipc_port: u16,
)
{
    unsafe {
        ASSET_MANAGER = {
            let aasset_manager = ndk_sys::AAssetManager_fromJava(env.get_raw(), asset_manager.as_raw());
            Some(AssetManager::from_ptr(
                std::ptr::NonNull::new(aasset_manager).unwrap()
            ))
        };
    };

    let manifest_file = env.get_string(&manifest_file).expect("invalid manifest file string").as_ptr();
    let ipaddr1_tx = env.get_string(&ipaddr1_tx).expect("invalid ipaddr1_tx string").as_ptr();
    let ipaddr1_rx = env.get_string(&ipaddr1_rx).expect("invalid ipaddr1_rx string").as_ptr();
    let ipaddr2_tx = env.get_string(&ipaddr2_tx).expect("invalid ipaddr2_tx string").as_ptr();
    let ipaddr2_rx = env.get_string(&ipaddr2_rx).expect("invalid ipaddr2_rx string").as_ptr();

    start_tx(manifest_file, ipaddr1_tx, ipaddr1_rx, ipaddr2_tx, ipaddr2_rx, duration, ipc_port);
}

#[cfg(target_os="android")]
#[allow(non_snake_case)]
#[allow(dead_code)]
pub extern "system" fn Java_com_github_magicsih_androidscreencaster_RustStreamReplay_start_rx(
    mut _env: JNIEnv, _: JClass,
    port: u16, duration: u32, calc_rtt: bool,
)
{
    start_rx(port, duration, calc_rtt);
}

#[cfg(target_os="android")]
#[allow(non_snake_case)]
#[allow(dead_code)]
pub extern "system" fn Java_com_github_magicsih_androidscreencaster_RustStreamReplay_send(
    mut env: JNIEnv, _: JClass,
    name: JString,
    buffer: JByteArray,
)
{
    let name: String = env.get_string(&name).expect("invalid name string").into();
    let sender = unsafe {
        SENDER_MAP.as_ref().unwrap().get(&name).unwrap()
    };

    let buffer: Vec<u8> = env.convert_byte_array(buffer).expect("invalid buffer array").to_vec();
    let mut packet = PacketStruct::new(0);
    packet.set_payload(&buffer);

    sender.send(packet).unwrap();
}
