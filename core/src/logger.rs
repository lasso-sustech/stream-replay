use log::{self, LevelFilter};
use env_logger::Builder;
use std::{env, fs::{self, OpenOptions}, io::Write};

fn get_log_level_from_env() -> LevelFilter {
    match env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()).to_lowercase().as_str() {
        "trace" => LevelFilter::Trace,
        "debug" => LevelFilter::Debug,
        "info" => LevelFilter::Info,
        "warn" => LevelFilter::Warn,
        "error" => LevelFilter::Error,
        _ => LevelFilter::Info,
    }
}

pub fn init_log(if_rx: bool) {
    let log_file = if if_rx { "log/rx_output.log" } else { "log/output.log" };
    let level = get_log_level_from_env();

    // Ensure log directory exists
    fs::create_dir_all("log").unwrap();

    // Clear the log file
    OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(log_file)
        .unwrap();

    // Set up env_logger to write to the log file
    let file = OpenOptions::new()
        .write(true)
        .append(true)
        .open(log_file)
        .unwrap();

    Builder::new()
        .format(move |_, record| {
            let log_line = format!("{} - {}", record.level(), record.args());
            let mut file = file.try_clone().expect("Failed to clone file handle");
            writeln!(file, "{}", log_line).expect("Failed to write to log file");
            Ok(())
        })
        .filter(None, level)
        .init();
}

#[cfg(target_os = "android")]
pub fn logging(s: &str) {
    use std::ffi::CString;
    let s = CString::new(s).unwrap();
    let tag = CString::new("RustStreamReplay").unwrap();
    unsafe {
        ndk_sys::__android_log_print(
            ndk_sys::android_LogPriority::ANDROID_LOG_INFO.0.try_into().unwrap(),
            tag.as_ptr(),
            s.as_ptr()
        );
    }
}
