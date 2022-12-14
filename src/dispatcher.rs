use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::net::UdpSocket;
use std::os::unix::io::AsRawFd;
use crate::packet::{PacketSender,PacketReceiver, PacketStruct};

pub type SourceInput = (PacketSender, BlockedSignal);
pub type BlockedSignal = Arc<Mutex<bool>>;

unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    ::std::slice::from_raw_parts(
        (p as *const T) as *const u8,
        ::std::mem::size_of::<T>(),
    )
}

unsafe fn set_tos(fd: i32, tos: u8) -> bool {
    let value = &(tos as i32) as *const libc::c_int as *const libc::c_void;
    let option_len = std::mem::size_of::<libc::c_int>() as u32;
    let res = libc::setsockopt(fd, libc::IPPROTO_IP, libc::IP_TOS, value, option_len);
    res == 0
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
        let fd = sock.as_raw_fd();
        assert!( set_tos(fd, tos) );
    }

    loop {
        // fetch bulky packets
        let packets:Vec<_> = rx.try_iter().collect();
        // send bulky packets aware of blocking status
        for packet in packets.iter() {
            let length = packet.length as usize;
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
