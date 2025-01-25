mod ebr;
mod gpt;
mod mbr;

use clap::{Arg, ArgAction, Command};
use ebr::{parse_ebr, print_ebr};
use exhume_body::Body;
use mbr::MBRPartitionEntry;
use std::io::Read;

fn process_file(file_path: &str, format: &str, verbose: &bool) {
    let mut body = Body::new(file_path.to_string(), format);
    if *verbose {
        body.print_info();
    }

    // Try to identify a MBR partition scheme
    let mut bootsector = vec![0u8; 512];
    // Read 512 bytes at ebr_absolute_lba
    body.read(&mut bootsector).unwrap();
    let main_mbr = mbr::MBR::from_bytes(&bootsector);
    let mut all_partitions: Vec<MBRPartitionEntry> = Vec::new();

    if main_mbr.is_mbr() {
        main_mbr.print_info();
        // EBR lookup
        for p in main_mbr.partition_table {
            // If it's an extended partition, parse the EBR chain
            match p.partition_type {
                0x05 | 0x0F | 0x85 => {
                    println!("Extended partition found !");
                    // p.start_lba is the LBA offset of the extended partition
                    let extended_partitions = parse_ebr(
                        &mut body,
                        p.start_lba,   // extended_base_lba
                        p.sector_size, // sector size
                    );
                    all_partitions.extend(extended_partitions);
                }
                _ => (), // not an extended partition
            };
        }
        print_ebr(&all_partitions);

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
