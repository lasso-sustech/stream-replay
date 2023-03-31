use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::net::UdpSocket;
use crate::packet::{PacketSender,PacketReceiver, PacketStruct, APP_HEADER_LENGTH, any_as_u8_slice};

pub type SourceInput = (PacketSender, BlockedSignal);
pub type BlockedSignal = Arc<Mutex<bool>>;

#[cfg(unix)]
fn create_udp_socket(tos: u8) -> Option<UdpSocket> {
    use std::os::unix::io::AsRawFd;

    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;

    let res = unsafe{
        let fd = sock.as_raw_fd();
        let value = &(tos as i32) as *const libc::c_int as *const libc::c_void;
        let option_len = std::mem::size_of::<libc::c_int>() as u32;
        libc::setsockopt(fd, libc::IPPROTO_IP, libc::IP_TOS, value, option_len)
    };
    
    if res == 0 { Some(sock) } else { None }
}

#[cfg(windows)]
fn create_udp_socket(tos: u8) -> Option<UdpSocket> {
    use std::net::Ipv4Addr;
    use std::os::windows::prelude::FromRawSocket;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Networking::WinSock::{socket,bind,WSADATA,WSAStartup,AF_INET,SOCK_DGRAM,IPPROTO_UDP, SOCKADDR_IN};
    use windows::Win32::Foundation::GetLastError;
    use windows::Win32::NetworkManagement::QoS::{QOS_VERSION, QOSCreateHandle, QOSAddSocketToFlow, QOSSetFlow};
    use windows::Win32::NetworkManagement::QoS::{QOS_SET_FLOW, QOSTrafficTypeBestEffort, QOS_NON_ADAPTIVE_FLOW, QOSSetOutgoingDSCPValue};

    unsafe fn print_error_message(prefix:&str) {
        use windows::core::PWSTR;
        use windows::Win32::System::Diagnostics::Debug::FormatMessageW;
        use windows::Win32::System::Diagnostics::Debug::FORMAT_MESSAGE_ALLOCATE_BUFFER;
        use windows::Win32::System::Diagnostics::Debug::{FORMAT_MESSAGE_FROM_SYSTEM,FORMAT_MESSAGE_IGNORE_INSERTS};

        let dw_flags = FORMAT_MESSAGE_ALLOCATE_BUFFER | FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS;
        let dw_message_id = GetLastError().0;
        let mut lp_buffer = PWSTR::null();

        FormatMessageW(dw_flags, None, dw_message_id, 0, PWSTR(&mut lp_buffer.0 as *mut _ as *mut _), 0, None);
        assert!( !lp_buffer.is_null() );
        eprintln!("{}, {}: {}", prefix, dw_message_id, lp_buffer.to_string().unwrap());
    }

    unsafe {
        let mut wsa = WSADATA::default();

        // Initialize WinSock
        if WSAStartup(0x0202, &mut wsa as *mut WSADATA)<0 {
            print_error_message("Initialize WinSock Failed");
            return None;
        }

        // Create socket
        let local_addr = "0.0.0.0".parse::<Ipv4Addr>().unwrap().into();
        let raw_sock = socket(AF_INET.0.into(), SOCK_DGRAM.into(), IPPROTO_UDP.0.into());
        let local_sockaddr = SOCKADDR_IN{ sin_family:AF_INET, sin_port:0, sin_addr:local_addr, ..Default::default() };
        bind(raw_sock, &local_sockaddr as *const _ as *const _, std::mem::size_of::<SOCKADDR_IN>() as i32);

        // Setup QoS
        let mut flow_id = 0;
        let mut qos_handle = HANDLE(0);
        let dscp_value = (tos >> 2) as u32; //DSCP value is the high-order 6 bits of the TOS
        let value_size = std::mem::size_of::<u32>();
        let qos_version = QOS_VERSION{ MajorVersion:1, MinorVersion:0 };

        if !QOSCreateHandle(&qos_version as *const _, &mut qos_handle as *mut HANDLE).as_bool() {
            print_error_message("QOSCreateHandle Failed");
            // return None;
        }

        if !QOSAddSocketToFlow(qos_handle, raw_sock, None, QOSTrafficTypeBestEffort, QOS_NON_ADAPTIVE_FLOW, &mut flow_id as *mut _).as_bool() {
            print_error_message("QOSAddSocketToFlow Failed");
            // return None;
        }

        if !QOSSetFlow(qos_handle,flow_id,QOSSetOutgoingDSCPValue as QOS_SET_FLOW,value_size as u32,&dscp_value as *const _ as *const _,0,None).as_bool() {
            print_error_message("QOSSetFlow Failed");
            // return None;
        }

        let sock = UdpSocket::from_raw_socket( raw_sock.0 as u64 );
        Some(sock)
    }
}

pub struct UdpDispatcher {
    pub records: Vec<SourceInput>,
    handles: Vec<JoinHandle<()>>
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

fn dispatcher_thread(rx: PacketReceiver, ipaddr:String, tos:u8, blocked_signal:BlockedSignal) {
    if let Some(sock) = create_udp_socket(tos) {
        sock.set_nonblocking(true).unwrap();

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
    else {
        let packets:Vec<_> = rx.try_iter().collect();
        for packet in packets.iter() {
            let port = packet.port;
            eprintln!("Socket creation failure: {}:{}@{}.", ipaddr, port, tos);
            break;
        }
    }
}
