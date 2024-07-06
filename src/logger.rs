use log::LevelFilter;
use log4rs::{
    append::file::FileAppender,
    config::{Appender, Config, Logger, Root},
    encode::pattern::PatternEncoder,
    init_config,
};
use std::{env, fs::File};

fn get_log_level_from_env() -> LevelFilter {
    match env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string()).to_lowercase().as_str() {
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
    let appender_name = if if_rx { "rx_file" } else { "file" };
    let level = get_log_level_from_env();

    File::create(log_file).unwrap(); //clear log

    let file_appender = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{l} - {m}{n}")))
        .build(log_file)
        .unwrap();

    let mut config_builder = Config::builder().appender(Appender::builder().build(appender_name, Box::new(file_appender)));

    if if_rx {
        config_builder = config_builder.logger(
            Logger::builder()
                .appender(appender_name)
                .additive(false)
                .build("destination", LevelFilter::Trace),
        );
    }

    let config = config_builder
        .build(Root::builder().appender(appender_name).build(level))
        .unwrap();

    init_config(config).unwrap();
}