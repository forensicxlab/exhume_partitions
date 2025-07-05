use clap::{value_parser, Arg, ArgAction, Command};
use exhume_body::Body;
use exhume_partitions::Partitions;
use log::{debug, error};
use std::fs;

fn process_file(
    file_path: &str,
    format: &str,
    json: bool,
    output: Option<&String>,
    bootloader: bool,
) {
    let mut body = Body::new(file_path.to_string(), format);
    debug!("Created Body from '{}'.", file_path);
    debug!("Discovering partitions.");
    match Partitions::new(&mut body) {
        Ok(partitions) => {
            let output_str = if json {
                serde_json::to_string_pretty(&partitions).unwrap()
            } else {
                partitions.print_info(bootloader)
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
        .version("0.2.4")
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
                .required(false)
                .help("The format of the file, either 'raw', 'ewf', or 'auto'."),
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
            Arg::new("bootloader")
                .long("bootloader")
                .action(ArgAction::SetTrue)
                .help("Display full MBR and potential EBR with bootstrap code"),
        )
        .arg(
            Arg::new("backup")
                .long("backup")
                .action(ArgAction::SetTrue)
                .help("Display the GPT backup if found"),
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
    let auto = String::from("auto");
    let format = matches.get_one::<String>("format").unwrap_or(&auto);
    let json = matches.get_flag("json");
    let output = matches.get_one::<String>("output");
    let bootloader = matches.get_flag("bootloader");
    process_file(file_path, format, json, output, bootloader);
}
