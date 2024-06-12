# TCP/UDP stream replay

### Requirements

- [Rust toolchain](https://www.rust-lang.org/learn/get-started)

- Python3, numpy

### Features

- Replay UDP streams from `*.npy` file.

- Specify streams configuration in `manifest.json` file.

- Support IPC for real-time monitor and control.

### How to use

**Tx:**
```bash
cargo run --bin stream-replay <manifest_file> <target_ip_address> <duration> [--ipc-port <IPC_PORT>]
```

**Rx:** 

Python-based reciever
```bash
python3 ./udp_rx.py -p <port> -t <duration> [--calc-jitter [--calc-rtt [--tos <TOS>]]]
```

Rust-based receiver
```bash
cargo run --bin stream-replay-rx <port> <duration> [calc-rtt]
```



### Screenshot

Real-time IPC throttle control of multiple live streams with RTT feedback.

![screenshot](previews/screenshot.png)
