pub mod ebr;
pub mod gpt;
pub mod mbr;

use byteorder::{LittleEndian, ReadBytesExt};
use ebr::print_info;
use exhume_body::Body;
use gpt::{format_guid, GPTPartitionEntry, GPT};
use log::{info, warn};
use mbr::{MBRPartitionEntry, MBR};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::io::{Cursor, Read, Seek};
const DEFAULT_SECTOR_SIZE: usize = 512;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Partitions {
    pub mbr: Option<MBR>,
    pub ebr: Option<Vec<MBRPartitionEntry>>,
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

        let gpt_record = match discover_gpt_partitions(body) {
            Ok(gpt) => Some(gpt),
            Err(_) => None,
        };

        Ok(Partitions {
            mbr: mbr_record,
            ebr: ebr_record,
            gpt: gpt_record,
        })
    }

    pub fn print_info(&self) -> String {
        let mut s = String::new();

        match &self.mbr {
            Some(mbr) => s.push_str(&mbr.print_info()),
            None => (),
        };
        s.push_str("\n");

        match &self.ebr {
            Some(ebr) => {
                if ebr.len() > 0 {
                    s.push_str(&print_info(&ebr))
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

fn discover_mbr_partitions(body: &mut Body) -> Result<mbr::MBR, Box<dyn Error>> {
    let mut bootsector = [0u8; DEFAULT_SECTOR_SIZE];
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

fn discover_ebr_partitions(body: &mut Body, main_mbr: &mbr::MBR) -> Vec<mbr::MBRPartitionEntry> {
    let mut all_partitions: Vec<mbr::MBRPartitionEntry> = Vec::new();
    for p in &main_mbr.partition_table {
        match p.partition_type {
            0x05 | 0x0F | 0x85 => {
                info!("Extended partition discovered.");
                let extended_partitions = ebr::parse_ebr(body, p.start_lba, p.sector_size);
                all_partitions.extend(extended_partitions);
            }
            _ => {}
        }
    }
    all_partitions
}

fn discover_gpt_partitions(body: &mut Body) -> Result<GPT, Box<dyn Error>> {
    let mut lba_1 = [0u8; 1024];
    body.seek(std::io::SeekFrom::Start(DEFAULT_SECTOR_SIZE as u64))
        .expect("Could not seek to lba_1 for GPT header parsing.");
    body.read(&mut lba_1)
        .expect("Could not read data from the source evidence.");
    let mut gpt = GPT::from_bytes(&lba_1);
    if gpt.is_gpt() {
        info!("Discovered a GPT partition scheme");
        // Now let's parse each entry using the header information
        body.seek(std::io::SeekFrom::Start(
            gpt.header.partition_entry_lba * DEFAULT_SECTOR_SIZE as u64,
        ))
        .expect("Could not seek to the GPT partition entry LBA");
        // Read Partition Entries (128 bytes each)
        let num_entries = gpt.header.num_partition_entries as usize;
        gpt.partition_entries = Vec::with_capacity(num_entries);

        let mut entry_data = vec![0u8; gpt.header.partition_entry_size as usize]; // Each GPT entry is 128 bytes
        for _ in 0..num_entries {
            body.read_exact(&mut entry_data)
                .expect("Could not read a partition entry.");
            let mut cursor = Cursor::new(&entry_data);

            let mut entry = GPTPartitionEntry::default();
            cursor
                .read_exact(&mut entry.partition_type_guid)
                .expect("Could not read the partition type GUID.");
            cursor
                .read_exact(&mut entry.partition_guid)
                .expect("Could not read the partition GUID.");
            entry.starting_lba = cursor
                .read_u64::<LittleEndian>()
                .expect("Could not read the starting LBA.");
            entry.ending_lba = cursor
                .read_u64::<LittleEndian>()
                .expect("Could not read the ending LBA.");
            entry.attributes = cursor
                .read_u64::<LittleEndian>()
                .expect("Could not read the GPT attributes");
            let mut buffer = vec![0u16; 36];
            cursor
                .read_u16_into::<byteorder::LittleEndian>(&mut buffer)
                .unwrap();
            entry.partition_name = String::from_utf16_lossy(&buffer);
            entry.description = entry.partition_type_description().to_string();
            entry.partition_type_guid_string = format_guid(&mut entry.partition_type_guid);
            entry.partition_guid_string = format_guid(&mut entry.partition_guid);
            gpt.partition_entries.push(entry);
        }
        Ok(gpt)
    } else {
        warn!("No GPT signature found");
        return Err("No GPT signature found".into());
    }
}
