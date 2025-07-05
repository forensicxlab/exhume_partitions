use crate::mbr::{MBRPartitionEntry, MBR};
use exhume_body::Body;
use prettytable::{Cell, Row, Table};
use std::io::{Read, Seek, SeekFrom};

pub fn parse_ebr(body: &mut Body, start_lba: u32, sector_size: usize) -> Vec<MBR> {
    let mut ebr_found = Vec::new();
    let ebr_absolute_lba = start_lba as usize * sector_size;
    body.seek(SeekFrom::Start(ebr_absolute_lba as u64)).unwrap();
    let mut ebr_data = vec![0u8; 512];
    body.read(&mut ebr_data).unwrap();
    let mut ebr = MBR::from_bytes(&ebr_data);
    let logical_partition = &mut ebr.partition_table[0];
    if logical_partition.partition_type != 0x00 {
        logical_partition.start_lba = start_lba + logical_partition.start_lba;
        logical_partition.first_byte_addr = logical_partition.start_lba as usize * sector_size;
    }
    let next_ebr_partition = &ebr.partition_table[1];
    if next_ebr_partition.partition_type != 0x00 {
        let next_ebr_start = next_ebr_partition.start_lba;
        ebr_found.extend(parse_ebr(body, next_ebr_start, sector_size));
    }
    ebr_found.push(ebr.clone());
    ebr_found
}

pub fn print_info(partitions: &Vec<MBRPartitionEntry>) -> String {
    let mut ebr_table = Table::new();
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

    ebr_table.add_row(Row::new(vec![
        Cell::new("Extented Boot Record Entries"),
        Cell::new(&partitions_table.to_string()),
    ]));
    ebr_table.to_string()
}
