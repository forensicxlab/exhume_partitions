use clap::*;
use exhume_body::{credentials, Body};
use exhume_partitions::Partitions;
use log::{debug, error};
use std::fs;
use std::process::ExitCode;

fn process_file(
    mut body: Body,
    json: bool,
    output: Option<&String>,
    bootloader: bool,
) -> Result<(), String> {
    debug!("Created Body from '{}'.", body.path);
    debug!("Discovering partitions.");
    let partitions = Partitions::new(&mut body)
        .map_err(|error| format!("could not discover partitions: {error}"))?;
    let output_str = if json {
        serde_json::to_string_pretty(&partitions)
            .map_err(|error| format!("cannot serialize partitions: {error}"))?
    } else {
        partitions.print_info(bootloader)
    };
    if let Some(output_path) = output {
        fs::write(output_path, output_str)
            .map_err(|error| format!("cannot write partition output: {error}"))?;
    } else {
        println!("{output_str}");
    }
    Ok(())
}

fn command() -> Command {
    password_args(
        Command::new("exhume_partitions")
            .version(crate_version!())
            .author(crate_authors!())
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
                    .default_value("auto")
                    .value_parser([
                        "raw", "vmdk", "aff", "aff4", "aff4l", "ewf", "qcow2", "vhdx", "auto",
                    ])
                    .help("The input image format."),
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
                Arg::new("output")
                    .long("output")
                    .value_parser(value_parser!(String))
                    .help("Output file path"),
            ),
    )
}

fn main() -> ExitCode {
    let matches = command().get_matches();
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
    let format = matches
        .get_one::<String>("format")
        .expect("defaulted by clap");
    let json = matches.get_flag("json");
    let output = matches.get_one::<String>("output");
    let bootloader = matches.get_flag("bootloader");
    match credentials::open_body(
        file_path,
        format,
        matches
            .get_one::<String>("password_file")
            .map(String::as_str),
        matches.get_flag("password_stdin"),
    )
    .and_then(|body| process_file(body, json, output, bootloader))
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            error!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// Register this executable's mutually exclusive container-password sources.
fn password_args(command: clap::Command) -> clap::Command {
    command
        .arg(
            clap::Arg::new("password_file")
                .long("password-file")
                .global(true)
                .value_name("PATH")
                .conflicts_with("password_stdin")
                .help("Read the image container password from a file; remove one trailing line ending."),
        )
        .arg(
            clap::Arg::new("password_stdin")
                .long("password-stdin")
                .global(true)
                .action(clap::ArgAction::SetTrue)
                .help("Read the image container password from stdin; remove one trailing line ending."),
        )
}
