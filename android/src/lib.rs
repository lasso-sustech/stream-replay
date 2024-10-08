#[cfg(target_os="android")]
#[allow(non_snake_case)]
pub mod android {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use serde_json;

    use core::packet::*;
    use core::logger::logging;
    use stream_replay_tx::conf::*;
    use stream_replay_tx::link::*;
    use stream_replay_tx::ipc::*;
    use stream_replay_tx::source::*;
    use stream_replay_rx::destination::*;
    use stream_replay_rx::record::*;

    use jni::JNIEnv;
    use jni::objects::{JClass, JObject, JString, JByteArray};
    use jni::sys::jbyteArray;
    use ndk::asset::AssetManager;

    type GuardedHashMap<K, V> = Arc<Mutex<HashMap<K, V>>>;
    type RegularBufferReceiver = std::sync::mpsc::Receiver<Vec<u8>>;

    pub static mut ASSET_MANAGER: Option<AssetManager> = None;
    pub static mut TX_SENDER_MAP: Option<HashMap<String, BufferSender>> = None;
    pub static mut RX_RECEIVER_MAP: Option<GuardedHashMap<u16, RegularBufferReceiver>> = None;

    fn start_tx(
        manifest_file: String,
        ipaddr1_tx: String, ipaddr1_rx: String,
        ipaddr2_tx: String, ipaddr2_rx: String,
        duration: f64,
        ipc_port: u16,
    )
    {
        unsafe {
            if let None = TX_SENDER_MAP {
                TX_SENDER_MAP = Some(HashMap::new());
            }
        }

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
                    param.links = vec![
                        Link{ tx_ipaddr: ipaddr1_tx.clone(), rx_ipaddr: ipaddr1_rx.clone() },
                        Link{ tx_ipaddr: ipaddr2_tx.clone(), rx_ipaddr: ipaddr2_rx.clone() }
                    ]
                }
            }
        });

        // parse the manifest file
        let streams:Vec<_> = manifest.streams.into_iter().filter_map( |x| x.validate(None, duration) ).collect();
        let window_size = manifest.window_size;
        println!("Sliding Window Size: {}.", window_size);

        // spawn the source thread
        let mut sources:HashMap<_,_> = streams.into_iter().map(|stream| {
            let src = SourceManager::new(stream, window_size);
            if !src.source.is_empty() {
                unsafe {
                    TX_SENDER_MAP.as_mut().unwrap().insert(src.name.clone(), src.source[0].clone());
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
        std::thread::spawn(move || {
            ipc.start_loop(duration);
        });
    }

    fn start_rx(
        port: u16,
        duration: u32,
        calc_rtt: bool,
        rx_mode: bool,
        src_ipaddrs: String,
    )
    {
        let src_ipaddrs = src_ipaddrs.split(",").map(|x| x.to_string()).collect();
        let args = Args { port, duration, calc_rtt, rx_mode, src_ipaddrs };
        let recv_data = Arc::new(Mutex::new(RecvData::new()));
        let recv_data_final = Arc::clone(&recv_data);

        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        recv_data.lock().unwrap().tx = Some(tx);

        // put the receiver into the global map
        unsafe {
            if let None = RX_RECEIVER_MAP {
                RX_RECEIVER_MAP = Some(Arc::new(Mutex::new(HashMap::new())));
            }
            RX_RECEIVER_MAP.as_ref().unwrap().lock().unwrap().insert(port, rx);
        }

        // Extract duration from args
        let duration = args.duration;
        let lock = Arc::new(Mutex::new(false));
        let lock_clone = Arc::clone(&lock);
        std::thread::spawn(move || {
            recv_thread(args, recv_data, lock_clone);
        });

        while !*lock.lock().unwrap() {
            std::thread::sleep(std::time::Duration::from_nanos(100_000) );
        }

        // Sleep for the duration
        std::thread::sleep(std::time::Duration::from_secs(duration as u64));

        let data_len = recv_data_final.lock().unwrap().data_len;
        let rx_duration = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64() - recv_data_final.lock().unwrap().rx_start_time;

        let recv_data = recv_data_final.lock().unwrap();
        let non_received = recv_data.last_seq - recv_data.recevied;

        logging( &format!("Received Bytes: {:.3} MB", data_len as f64/ 1024.0 / 1024.0) );
        logging( &format!("Average Throughput: {:.3} Mbps", data_len as f64 / rx_duration / 1e6 * 8.0) );
        logging( &format!("Packet loss rate: {:.5}", non_received as f64 / recv_data.last_seq as f64) );
        logging( &format!("Stuttering rate: {:.5}", recv_data.stutter.get_stuttering()) );
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_lasso_1sustech_androidscreencaster_service_RustStreamReplay_startTx(
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

        let manifest_file = env.get_string(&manifest_file).expect("invalid manifest file string").into();
        let ipaddr1_tx = env.get_string(&ipaddr1_tx).expect("invalid ipaddr1_tx string").into();
        let ipaddr1_rx = env.get_string(&ipaddr1_rx).expect("invalid ipaddr1_rx string").into();

        let ipaddr2_tx = env.get_string(&ipaddr2_tx).expect("invalid ipaddr2_tx string").into();
        let ipaddr2_rx = env.get_string(&ipaddr2_rx).expect("invalid ipaddr2_rx string").into();

        start_tx(manifest_file, ipaddr1_tx, ipaddr1_rx, ipaddr2_tx, ipaddr2_rx, duration, ipc_port);
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_lasso_1sustech_androidscreencaster_service_RustStreamReplay_startRx(
        mut _env: JNIEnv, _: JClass,
        port: u16, duration: u32, calc_rtt: bool, rx_mode: bool, src_ipaddrs: JString
    )
    {
        let src_ipaddrs = _env.get_string(&src_ipaddrs).expect("invalid src_ipaddrs string").into();
        std::thread::spawn(move || {
            start_rx(port, duration, calc_rtt, rx_mode, src_ipaddrs);
        });
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_lasso_1sustech_androidscreencaster_service_RustStreamReplay_send(
        mut env: JNIEnv, _: JClass,
        name: JString,
        buffer: JByteArray,
    )
    {
        let name: String = env.get_string(&name).expect("invalid name string").into();
        let sender = unsafe {
            TX_SENDER_MAP.as_ref().unwrap().get(&name).unwrap()
        };

        let buffer: Vec<u8> = env.convert_byte_array(buffer).expect("invalid buffer array").to_vec();

        match sender.send(buffer) {
            Ok(_) => {},
            Err(e) => {
                logging(&format!("Error sending buffer: {:?}", e));
            }
        }
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_lasso_1sustech_androidscreencaster_service_RustStreamReplay_recv(
        env: JNIEnv, _: JClass,
        port: u16,
    ) -> jbyteArray
    {
        let empty_array = env.new_byte_array(0).expect("invalid byte array");

        match unsafe{ RX_RECEIVER_MAP.as_ref() } {
            None => empty_array.into_raw(),
            Some(rx_map) => {
                match rx_map.lock().expect("rx_map lock failed").get(&port) {
                    None => empty_array.into_raw(),
                    Some(rx) => {
                        // let array: Vec<u8> = rx.try_iter().flatten().collect();
                        let array: Vec<u8> = rx.try_recv().unwrap_or(vec![]);
                        let array = env.byte_array_from_slice(&array).expect("invalid byte array");
                        array.into_raw()
                    }
                }
            }
        }
    }

}
