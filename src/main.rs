use clap::{value_parser, Arg, ArgAction, Command};
use exhume_body::Body;
use exhume_partitions::Partitions;
use log::{debug, error};
use std::fs;

fn process_file(file_path: &str, format: &str, json: bool, output: Option<&String>) {
    let mut body = Body::new(file_path.to_string(), format);
    debug!("Created Body from '{}'.", file_path);
    debug!("Discovering partitions.");
    match Partitions::new(&mut body) {
        Ok(partitions) => {
            let output_str = if json {
                serde_json::to_string_pretty(&partitions).unwrap()
            } else {
                partitions.to_output_string()
            };
            if let Some(output_path) = output {
                fs::write(output_path, output_str).unwrap();
            } else {
                println!("{}", output_str);
            }
        }
        Err(err) => {
            error!("Could not discover partitions: {:?}", err);
        }
    }
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
        .arg(
            Arg::new("json")
                .long("json")
                .action(ArgAction::SetTrue)
                .help("Display partitions in JSON format"),
        )
        .arg(
            Arg::new("output")
                .long("output")
                .value_parser(value_parser!(String))
                .help("Output file path"),
        )
        .get_matches();

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
    let json = matches.get_flag("json");
    let output = matches.get_one::<String>("output");
    process_file(file_path, format, json, output);
}
