mod gpt;
mod mbr;

use clap::{Arg, ArgAction, Command};
use exhume_body::Body;

fn process_file(file_path: &str, format: &str, verbose: &bool) {
    let mut body = Body::new(file_path.to_string(), format);
    if *verbose {
        body.print_info();
    }

    // Try to identify a MBR partition scheme
    let bootsector = body.read(512);
    let potential_mbr = mbr::MBR::from_bytes(&bootsector);

    if potential_mbr.is_mbr() {
        potential_mbr.print_info();
        // TODO: Handle the logic to parse GPT if the partition type found is GPT.
        // I need to find a disk image using GPT partition scheme.
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
                .value_parser(clap::value_parser!(String))
                .required(true)
                .help("The path to the body to exhume."),
        )
        .arg(
            Arg::new("format")
                .short('f')
                .long("format")
                .value_parser(clap::value_parser!(String))
                .required(true)
                .help("The format of the file, either 'raw' or 'ewf'."),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .action(ArgAction::SetTrue),
        )
        .get_matches();

    let file_path = matches.get_one::<String>("body").unwrap();
    let format = matches.get_one::<String>("format").unwrap();
    let verbose = match matches.get_one::<bool>("verbose") {
        Some(verbose) => verbose,
        None => &false,
    };
    process_file(file_path, format, verbose);
}
