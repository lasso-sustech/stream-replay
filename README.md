# TCP/UDP stream replay

### Requirement

- Linux platform

- Python 3

- Rust toolchain

### How to use

```bash
cargo run -- <manifest_file> <target_ip_address>
```

### Features

- Read replay configuration from `manifest.json` file;

- Support concurrent TCP/UDP stream replay;

- Easily manipulate data trace with `npy` file.
