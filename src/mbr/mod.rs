// Reference: https://en.wikipedia.org/wiki/Master_boot_record

use byteorder::{LittleEndian, ReadBytesExt};
use capstone::prelude::*;
use prettytable::{Cell, Row, Table};
use std::io::{Cursor, Read};

/// A MBR Partition Entry (16 bytes)
#[derive(Debug, Default)]
pub struct MBRPartitionEntry {
    boot_indicator: u8, // Bootable (0x80 for active, 0x00 for inactive)
    start_chs: [u8; 3], // CHS (Cylinder-Head-Sector) address of the start of the partition
    partition_type: u8, // Partition type (e.g., 0x07 for NTFS, 0x83 for Linux)
    end_chs: [u8; 3],   // CHS address of the end of the partition
    start_lba: u32,     // Start address of the partition in LBA (Logical Block Addressing)
    size_sectors: u32,  // Size of the partition in sectors
}

/// MBR Structure (512 bytes)
#[derive(Debug)]
pub struct MBR {
    bootloader: [u8; 446],                   // Bootloader code (size: 446 bytes)
    partition_table: [MBRPartitionEntry; 4], // Partition table (max 4 entries)
    boot_signature: u16,                     // the value should be 0x55AA
}

impl MBR {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut cursor = Cursor::new(bytes);

        let mut mbr = MBR {
            bootloader: [0u8; 446],              // Initialize bootloader array with zeros
            partition_table: Default::default(), // Initialize partition table with default values
            boot_signature: 0,                   // Initialize signature with 0
        };

        if bytes.len() < 512 {
            eprintln!("512 bytes are required to identify an MBR");
            std::process::exit(1);
        }

        // Read bootloader (446 bytes)
        cursor.read_exact(&mut mbr.bootloader).unwrap();

        // Read partition table (4 entries, each 16 bytes)
        for i in 0..4 {
            mbr.partition_table[i] = MBRPartitionEntry {
                boot_indicator: cursor.read_u8().unwrap(),
                start_chs: [
                    cursor.read_u8().unwrap(),
                    cursor.read_u8().unwrap(),
                    cursor.read_u8().unwrap(),
                ],
                partition_type: cursor.read_u8().unwrap(),
                end_chs: [
                    cursor.read_u8().unwrap(),
                    cursor.read_u8().unwrap(),
                    cursor.read_u8().unwrap(),
                ],
                start_lba: cursor.read_u32::<LittleEndian>().unwrap(),
                size_sectors: cursor.read_u32::<LittleEndian>().unwrap(),
            };
        }

        // Read MBR signature (last 2 bytes)
        // Since little-endian representation must be assumed in the context of IBM PC compatible machines,
        // this can be written as 16-bit word 'AA55'hex in programs for x86 processors (note the swapped order),
        // whereas it would have to be written as '55AA'hex in programs for other CPU architectures using a big-endian representation.
        // Here we choose to use LittleEndian so 'AA55'hex should match.
        mbr.boot_signature = cursor.read_u16::<LittleEndian>().unwrap();

        mbr
    }

    pub fn is_mbr(&self) -> bool {
        let mbr_signature = 0xAA55;
        self.boot_signature == mbr_signature
    }

    pub fn print_info(&self) {
        println!("The disk uses the MBR partition scheme:");
        let mut mbr_table = Table::new();
        let mut partitions_table = Table::new();

        let cs = Capstone::new()
            .x86()
            .mode(arch::x86::ArchMode::Mode16) // Use 16-bit mode
            .build()
            .unwrap();

        let instructions = cs.disasm_all(&self.bootloader, 0x1000).unwrap();
        let opcodes: Vec<String> = instructions
            .iter()
            .map(|ins| ins.to_string()) // Converts each instruction to a string
            .collect();
        let opcodes_str = opcodes.join("\n");

        mbr_table.add_row(Row::new(vec![
            Cell::new("Bootloader"),
            Cell::new(&opcodes_str),
        ]));

        // Now, we create a table for each partitions
        partitions_table.add_row(Row::new(vec![
            Cell::new("Bootable"),
            Cell::new("Start address (CHS)"),
            Cell::new("End address (CHS)"),
            Cell::new("Partition type"),
            Cell::new("Start address (LBA)"),
            Cell::new("Size (in sectors)"),
        ]));

        for partition in &self.partition_table {
            let start_chs_number: u32 = ((partition.start_chs[0] as u32) << 16)
                | ((partition.start_chs[1] as u32) << 8)
                | (partition.start_chs[2] as u32);

            let end_chs_number: u32 = ((partition.end_chs[0] as u32) << 16)
                | ((partition.end_chs[1] as u32) << 8)
                | (partition.end_chs[2] as u32);

            partitions_table.add_row(Row::new(vec![
                Cell::new(&(format!("0x{:02x}", partition.boot_indicator))),
                Cell::new(&(format!("0x{:06x}", start_chs_number))),
                Cell::new(&(format!("0x{:06x}", end_chs_number))),
                Cell::new(&(format!("0x{:x}", partition.partition_type))),
                Cell::new(&(format!("0x{:x}", partition.start_lba))),
                Cell::new(&(format!("0x{:x}", partition.size_sectors))),
            ]));
        }

        mbr_table.add_row(Row::new(vec![
            Cell::new("Partition tables entries"),
            Cell::new(&partitions_table.to_string()),
        ]));

        mbr_table.add_row(Row::new(vec![
            Cell::new("MBR Signature"),
            Cell::new(&(format!("0x{:x}", self.boot_signature))),
        ]));

        mbr_table.printstd();
    }
}
