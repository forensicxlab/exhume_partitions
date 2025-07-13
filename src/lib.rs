pub mod ebr;
pub mod gpt;
pub mod mbr;

use exhume_body::Body;
use gpt::{GPTHeader, GPTPartitionEntry, GPT};
use log::{error, info, warn};
use mbr::MBR;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::io::{Read, Seek, SeekFrom};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Partitions {
    pub mbr: Option<MBR>,
    pub ebr: Option<Vec<MBR>>,
    pub gpt: Option<GPT>,
}

impl Partitions {
    pub fn new(body: &mut Body) -> Result<Partitions, Box<dyn Error>> {
        let mbr_record = match discover_mbr_partitions(body) {
            Ok(mbr) => Some(mbr),
            Err(msg) => {
                warn!("No MBR Found: {:?}", msg);
                None
            }
        };

        let ebr_record = match &mbr_record {
            Some(mbr) => Some(discover_ebr_partitions(body, mbr)),
            None => None,
        };

        let gpt_record = match discover_any_gpt(body) {
            Ok(gpt) => Some(gpt),
            Err(e) => {
                warn!("No GPT Found: {:?}", e);
                None
            }
        };

        Ok(Partitions {
            mbr: mbr_record,
            ebr: ebr_record,
            gpt: gpt_record,
        })
    }

    pub fn print_info(&self, bootloader: bool) -> String {
        let mut s = String::new();

        match &self.mbr {
            Some(mbr) => s.push_str(&mbr.print_info(&bootloader)),
            None => (),
        };
        s.push_str("\n");

        match &self.ebr {
            Some(ebr) => {
                if ebr.len() > 0 {
                    for ebr_entry in ebr {
                        s.push_str(&ebr_entry.print_info(&bootloader))
                    }
                }
            }
            None => (),
        };

        match &self.gpt {
            Some(gpt) => s.push_str(&gpt.print_info()),
            None => (),
        };

        s
    }
}

fn discover_any_gpt(body: &mut Body) -> Result<GPT, Box<dyn Error>> {
    discover_gpt_partitions(body, false) // primary @ LBA 1
        .or_else(|_| discover_gpt_partitions(body, true)) // backup @ last LBA
}

fn discover_mbr_partitions(body: &mut Body) -> Result<mbr::MBR, Box<dyn Error>> {
    let mut bootsector: Vec<u8> = vec![0; body.get_sector_size() as usize];
    body.read(&mut bootsector).unwrap();
    let main_mbr = mbr::MBR::from_bytes(&bootsector);
    if main_mbr.is_mbr() {
        info!("Detected an MBR partition scheme.");
        if main_mbr.is_pmbr() {
            info!("Detected a Protective MBR. GPT scheme is strongly suspected.")
        }
        Ok(main_mbr)
    } else {
        warn!("No MBR signature found");
        return Err("No MBR signature found".into());
    }
}

pub fn read_gpt_header_at(body: &mut Body, lba: u64) -> Result<GPTHeader, Box<dyn Error>> {
    let sector_size = body.get_sector_size() as u64;
    let mut hdr_bytes = [0u8; 92];
    body.seek(SeekFrom::Start(lba * sector_size))?;
    body.read_exact(&mut hdr_bytes)?;
    let gpt = GPT::from_bytes(&hdr_bytes);
    if gpt.is_gpt() {
        Ok(gpt.header)
    } else {
        error!("No GPT signature found at requested LBA");
        Err("No GPT signature found at requested LBA".into())
    }
}

fn discover_ebr_partitions(body: &mut Body, main_mbr: &mbr::MBR) -> Vec<MBR> {
    let mut all_partitions: Vec<MBR> = Vec::new();
    for p in &main_mbr.partition_table {
        match p.partition_type {
            0x05 | 0x0F | 0x85 => {
                info!("Extended Boot Record (EBR) partition discovered.");
                let extended_partitions = ebr::parse_ebr(body, p.start_lba, p.sector_size);
                all_partitions.extend(extended_partitions);
            }
            _ => {}
        }
    }
    all_partitions
}

/// Read the GPT (primary or backup) and all of its partition-table entries.
/// Parse the primary header at LBA 1 or parse the backup header at the last LBA of the image.
fn discover_gpt_partitions(body: &mut Body, backup: bool) -> Result<GPT, Box<dyn Error>> {
    let sector_size = body.get_sector_size() as u64;

    // Primary GPT header is always at LBA 1
    // Backup GPT header is always at the last LBA of the disk / image
    let target_lba = if backup {
        let end_offset = body.seek(SeekFrom::End(0))?; // byte position == file length
        if end_offset < sector_size {
            return Err("Evidence is smaller than one sector".into());
        }
        (end_offset / sector_size) - 1 // last LBA
    } else {
        1 // primary header
    };

    let mut hdr_raw = vec![0u8; sector_size as usize];
    body.seek(SeekFrom::Start(target_lba * sector_size))?;
    body.read_exact(&mut hdr_raw)?;

    let mut gpt = GPT::from_bytes(&hdr_raw);

    if !gpt.is_gpt() {
        return Err("Invalid Signature".into());
    }

    info!(
        "Discovered a {} GPT header at LBA {}",
        if backup { "backup" } else { "primary" },
        target_lba
    );

    body.seek(SeekFrom::Start(
        gpt.header.partition_entry_lba * sector_size,
    ))?;

    let num_entries = gpt.header.num_partition_entries as usize;
    let entry_size = gpt.header.partition_entry_size as usize;
    let mut entry_buf = vec![0u8; entry_size];

    gpt.partition_entries = Vec::with_capacity(num_entries);

    for _ in 0..num_entries {
        body.read_exact(&mut entry_buf)?;
        let entry = GPTPartitionEntry::from_bytes(&entry_buf);

        // Skip unused (all-zero) entries to keep the output tidy
        if entry.partition_type_guid != [0u8; 16] {
            gpt.partition_entries.push(entry);
        }
    }

    Ok(gpt)
}
