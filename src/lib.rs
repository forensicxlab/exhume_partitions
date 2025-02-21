pub mod ebr;
pub mod gpt;
pub mod mbr;

use exhume_body::Body;
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::io::Read;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Partitions {
    pub mbr: mbr::MBR,
    pub ebr: Vec<mbr::MBRPartitionEntry>,
    //TODO: add gpt partitions once implemented
}

impl Partitions {
    pub fn new(body: &mut Body) -> Result<Partitions, Box<dyn Error>> {
        let mbr_record = match discover_mbr_partitions(body) {
            Ok(mbr_record) => mbr_record,
            Err(err) => return Err(err),
        };
        let ebr_record = discover_ebr_partitions(body, &mbr_record);
        Ok(Partitions {
            mbr: mbr_record,
            ebr: ebr_record,
        })
    }
    pub fn print_info(&self) {
        self.mbr.print_info();
        ebr::print_ebr(&self.ebr);
    }
}

fn discover_mbr_partitions(body: &mut Body) -> Result<mbr::MBR, Box<dyn Error>> {
    // Try to identify an MBR partition scheme.
    let mut bootsector = vec![0u8; 512];
    body.read(&mut bootsector).unwrap();
    let main_mbr = mbr::MBR::from_bytes(&bootsector);
    if main_mbr.is_mbr() {
        Ok(main_mbr)
    } else {
        error!("No MBR found");
        return Err("No MBR found".into());
    }
}

fn discover_ebr_partitions(body: &mut Body, main_mbr: &mbr::MBR) -> Vec<mbr::MBRPartitionEntry> {
    let mut all_partitions: Vec<mbr::MBRPartitionEntry> = Vec::new();
    for p in &main_mbr.partition_table {
        // If it’s an extended partition, parse the EBR chain.
        match p.partition_type {
            0x05 | 0x0F | 0x85 => {
                info!("Extended partition found.");
                let extended_partitions = ebr::parse_ebr(
                    body,
                    p.start_lba,   // extended_base_lba
                    p.sector_size, // sector size
                );
                all_partitions.extend(extended_partitions);
            }
            _ => {}
        }
    }
    all_partitions
}
