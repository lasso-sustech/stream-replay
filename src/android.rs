use std::os::raw::c_char;
use std::ffi::CStr;
use std::collections::HashMap;
use serde_json;

use crate::conf::Manifest;
use crate::conf::StreamParam;
use crate::ipc::IPCDaemon;
use crate::source::SourceManager;
use crate::broker::GlobalBroker;

use jni::JNIEnv;
use jni::objects::{JClass, JObject, JString};

use ndk::asset::AssetManager;

pub static mut ASSET_MANAGER: Option<AssetManager> = None;

#[cfg(target_os="android")]
#[no_mangle]
pub extern "C" fn start(
    manifest_file: *const c_char,
    ipaddr1: *const c_char,
    ipaddr2: *const c_char,
    duration: f64,
    ipc_port: u16,
)
{
    let manifest_file: String = unsafe { CStr::from_ptr(manifest_file).to_string_lossy().into_owned() };
    let ipaddr1: String = unsafe { CStr::from_ptr(ipaddr1).to_string_lossy().into_owned() };
    let ipaddr2: String = unsafe { CStr::from_ptr(ipaddr2).to_string_lossy().into_owned() };

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
    manifest.tx_ipaddrs = vec![ipaddr1.clone(), ipaddr2.clone()];
    manifest.streams.iter_mut().for_each(|stream| {
        match stream {
            StreamParam::TCP(param) => {
                param.tx_ipaddrs = vec![ipaddr1.clone(), ipaddr2.clone()];
            },
            StreamParam::UDP(param) => {
                param.tx_ipaddrs = vec![ipaddr1.clone(), ipaddr2.clone()];
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
    let mut broker = GlobalBroker::new( orchestrator, ipaddr1, port2ip);
    let _handle = broker.start();

    // spawn the source thread
    let mut sources:HashMap<_,_> = streams.into_iter().map(|stream| {
        let src = SourceManager::new(stream, window_size, &mut broker);
        let name = src.name.clone();
        (name, src)
    }).collect();
    let _handles:Vec<_> = sources.iter_mut().enumerate().map(|(i,(_name,src))| {
        src.start(i+1, String::from("0.0.0.0"))
    }).collect();

    // start global IPC
    let ipc = IPCDaemon::new( sources, ipc_port, String::from("0.0.0.0"));
    ipc.start_loop( duration);

    std::process::exit(0); //force exit
}

#[cfg(target_os="android")]
#[allow(non_snake_case)]
#[allow(dead_code)]
pub extern "system" fn Java_com_github_magicsih_androidscreencaster_RustStreamReplay_start(
    mut env: JNIEnv, _: JClass, asset_manager: JObject,
    manifest_file: JString,
    ipaddr1: JString,
    ipaddr2: JString,
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
    let ipaddr1 = env.get_string(&ipaddr1).expect("invalid ipaddr1 string").as_ptr();
    let ipaddr2 = env.get_string(&ipaddr2).expect("invalid ipaddr2 string").as_ptr();

    start(manifest_file, ipaddr1, ipaddr2, duration, ipc_port);
}
