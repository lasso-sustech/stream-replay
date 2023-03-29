use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::net::UdpSocket;
use crate::packet::{PacketSender,PacketReceiver, PacketStruct, APP_HEADER_LENGTH, any_as_u8_slice};

pub type SourceInput = (PacketSender, BlockedSignal);
pub type BlockedSignal = Arc<Mutex<bool>>;

#[cfg(unix)]
unsafe fn set_tos(sock: &UdpSocket, tos: u8) -> bool {
    use std::os::unix::io::AsRawFd;

    let fd = sock.as_raw_fd();
    let value = &(tos as i32) as *const libc::c_int as *const libc::c_void;
    let option_len = std::mem::size_of::<libc::c_int>() as u32;
    let res = libc::setsockopt(fd, libc::IPPROTO_IP, libc::IP_TOS, value, option_len);
    res == 0
}

#[cfg(windows)]
unsafe fn set_tos(sock: &UdpSocket, tos: u8) -> bool {
    use std::os::windows::io::AsRawSocket;
    use core::ffi::c_void;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Networking::WinSock::SOCKET;
    use windows::Win32::NetworkManagement::QoS::{QOS_VERSION, QOSCreateHandle, QOSAddSocketToFlow, QOSSetFlow};
    use windows::Win32::NetworkManagement::QoS::{QOS_SET_FLOW, QOSTrafficTypeBestEffort, QOS_NON_ADAPTIVE_FLOW, QOSSetOutgoingDSCPValue};

    let raw_sock = SOCKET(sock.as_raw_socket() as usize);
    let mut flow_id = 0;
    let mut qos_handle = HANDLE(0);
    let dscp_value = (tos >> 2) as u32; //DSCP value is the high-order 6 bits of the TOS
    let value_size = std::mem::size_of::<u32>();
    let qos_version = QOS_VERSION{ MajorVersion:1, MinorVersion:0 };

    if !QOSCreateHandle(&qos_version as *const _, &mut qos_handle as *mut HANDLE).as_bool() {
        println!("QOSCreateHandle failed.");
        return false;
    }

    if !QOSAddSocketToFlow(qos_handle, raw_sock, None, QOSTrafficTypeBestEffort, QOS_NON_ADAPTIVE_FLOW, &mut flow_id as *mut _ as *mut u32).as_bool() {
        println!("QOSAddSocketToFlow failed.");
        return false;
    }

    if !QOSSetFlow(qos_handle,flow_id,QOSSetOutgoingDSCPValue as QOS_SET_FLOW,value_size as u32,&dscp_value as *const _ as *const c_void,0,None).as_bool() {
        println!("QOSSetFlow failed.");
        return false;
    }

    true
}

pub struct UdpDispatcher {
    pub records: Vec<SourceInput>,
    handles: Vec<JoinHandle<Result<(), std::io::Error>>>
}

impl UdpDispatcher {
    pub fn new() -> Self {
        let records = Vec::new();
        let handles = Vec::new();

        Self { records, handles }
    }

    pub fn start_new(&mut self, ipaddr:String, tos:u8) -> SourceInput {
        let (tx, rx) = mpsc::channel::<PacketStruct>();
        let blocked_signal:BlockedSignal = Arc::new(Mutex::new(false));
        let cloned_blocked_signal = Arc::clone(&blocked_signal);
        let handle = thread::spawn(move || {
            dispatcher_thread(rx, ipaddr, tos, blocked_signal)
        });

        let res = ( tx.clone(), Arc::clone(&cloned_blocked_signal) );
        self.records.push( (tx, cloned_blocked_signal) );
        self.handles.push( handle );

        res
    }

    pub fn start_agg_sockets(&mut self, ipaddr:String) {
        let ipaddr_list = std::iter::repeat(ipaddr);
        let tos_list = [200, 150, 100, 50];
        let _:Vec<_> = std::iter::zip(ipaddr_list, tos_list).map(
            |(ipaddr, tos)| {
                self.start_new(ipaddr, tos)
            }
        ).collect(); //discard the cloned responses
    }

}

fn dispatcher_thread(rx: PacketReceiver, ipaddr:String, tos:u8, blocked_signal:BlockedSignal) -> Result<(), std::io::Error> {
    let sock = UdpSocket::bind("0.0.0.0:0")?;
    sock.set_nonblocking(true).unwrap();

    unsafe {
        assert!( set_tos(&sock, tos) );
    }

    loop {
        // fetch bulky packets
        let packets:Vec<_> = rx.try_iter().collect();
        // send bulky packets aware of blocking status
        for packet in packets.iter() {
            let length = packet.length as usize;
            let length = std::cmp::max(length, APP_HEADER_LENGTH);
            let buf = unsafe{ any_as_u8_slice(packet) };
            loop {
                let port = packet.port;
                let addr = format!("{}:{}", ipaddr, port);
                let mut _signal = blocked_signal.lock().unwrap();
                match sock.send_to(&buf[..length], &addr) {
                    Ok(_len) => {
                        *_signal = false;
                        break
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        *_signal = true;
                        continue // block occurs
                    }
                    Err(e) => panic!("encountered IO error: {e}")
                }
            }             
        }
    }
}
