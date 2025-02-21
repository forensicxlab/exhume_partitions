use crate::mbr::{MBRPartitionEntry, MBR};
use exhume_body::Body;
use prettytable::{Cell, Row, Table};
use std::io::{Read, Seek, SeekFrom};

/// Recursively parse Extended Boot Records (EBRs) and discover all logical partitions.
///
/// * `disk_data`: the full disk image in memory
/// * `ebr_relative_lba`: the LBA offset of the *current* EBR, relative to the extended partition base
/// * `extended_base_lba`: the LBA where the extended partition itself begins (from the MBR).
/// * `sector_size`: typically 512
///
/// Returns a vector of MBRPartitionEntry discovered in the extended partition chain.
pub fn parse_ebr(body: &mut Body, start_lba: u32, sector_size: usize) -> Vec<MBRPartitionEntry> {
    let mut partitions_found = Vec::new();
    let ebr_absolute_lba = start_lba as usize * sector_size;
    body.seek(SeekFrom::Start(ebr_absolute_lba as u64)).unwrap();
    let mut ebr_data = vec![0u8; 512];

    // Read 512 bytes at ebr_absolute_lba
    body.read(&mut ebr_data).unwrap();

    let mut ebr = MBR::from_bytes(&ebr_data);

    // The first partition in the EBR is the logical partition
    let logical_partition = &mut ebr.partition_table[0];
    if logical_partition.partition_type != 0x00 {
        // A valid partition entry
        logical_partition.start_lba = start_lba + logical_partition.start_lba;
        logical_partition.first_byte_addr = logical_partition.start_lba as usize * sector_size;
        partitions_found.push(logical_partition.clone());
    }

    // The second partition in the EBR points to the *next* EBR, if any
    let next_ebr_partition = &ebr.partition_table[1];
    if next_ebr_partition.partition_type != 0x00 {
        // The "start_lba" of this partition is relative to the *extended partition base*.
        let next_ebr_start = next_ebr_partition.start_lba;
        // Recursively parse the next EBR and append the discovered partitions
        let mut further = parse_ebr(body, next_ebr_start, sector_size);
        partitions_found.append(&mut further);
    }

    partitions_found
}

pub fn print_ebr(partitions: &Vec<MBRPartitionEntry>) {
    let mut partitions_table = Table::new();
    partitions_table.add_row(Row::new(vec![
        Cell::new("Bootable"),
        Cell::new("Start (CHS)"),
        Cell::new("End (CHS)"),
        Cell::new("Start (LBA)"),
        Cell::new("Type"),
        Cell::new("Description"),
        Cell::new("First Byte Addr"),
        Cell::new("Size (sectors)"),
    ]));

    for partition in partitions {
        partitions_table.add_row(Row::new(vec![
            Cell::new(&format!("0x{:02X}", partition.boot_indicator)),
            Cell::new(&format!("{:?}", partition.start_chs_tuple())),
            Cell::new(&format!("{:?}", partition.end_chs_tuple())),
            Cell::new(&format!("0x{:X}", partition.start_lba)),
            Cell::new(&format!("0x{:02X}", partition.partition_type)),
            Cell::new(&format!("{:?}", partition.description)),
            Cell::new(&format!("0x{:X}", partition.first_byte_addr)),
            Cell::new(&format!("0x{:X}", partition.size_sectors)),
        ]));
    }

    partitions_table.printstd();
}
