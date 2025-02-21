use clap::{value_parser, Arg, Command};
use exhume_body::Body;
use exhume_partitions::Partitions;
use log::{debug, error};

fn process_file(file_path: &str, format: &str) {
    let mut body = Body::new(file_path.to_string(), format);
    // Instead of conditionally printing body info, log it at debug level.
    debug!("Created Body from '{}'.", file_path);
    debug!("Discovering partitions.");
    match Partitions::new(&mut body) {
        Ok(discover_partitions) => {
            discover_partitions.print_info();
        }
        Err(err) => {
            error!("Could not discover partitions: {:?}", err);
        }
    };
}

fn main() {
    let matches = Command::new("exhume_partitions")
        .version("1.0")
        .author("ForensicXlab")
        .about("Exhume the partitions from a given body of data.")
        .arg(
            Arg::new("body")
                .short('b')
                .long("body")
                .value_parser(value_parser!(String))
                .required(true)
                .help("The path to the body to exhume."),
        )
        .arg(
            Arg::new("format")
                .short('f')
                .long("format")
                .value_parser(value_parser!(String))
                .required(true)
                .help("The format of the file, either 'raw' or 'ewf'."),
        )
        .arg(
            Arg::new("log_level")
                .short('l')
                .long("log-level")
                .value_parser(["error", "warn", "info", "debug", "trace"])
                .default_value("info")
                .help("Set the log verbosity level"),
        )
        .get_matches();

    // Initialize the logger.
    let log_level_str = matches.get_one::<String>("log_level").unwrap();
    let level_filter = match log_level_str.as_str() {
        "error" => log::LevelFilter::Error,
        "warn" => log::LevelFilter::Warn,
        "info" => log::LevelFilter::Info,
        "debug" => log::LevelFilter::Debug,
        "trace" => log::LevelFilter::Trace,
        _ => log::LevelFilter::Info,
    };
    env_logger::Builder::new().filter_level(level_filter).init();

    let file_path = matches.get_one::<String>("body").unwrap();
    let format = matches.get_one::<String>("format").unwrap();
    process_file(file_path, format);
}
