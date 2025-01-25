// Reference: https://en.wikipedia.org/wiki/Master_boot_record

use byteorder::{LittleEndian, ReadBytesExt};
use capstone::prelude::*;
use prettytable::{Cell, Row, Table};
use std::io::{Cursor, Read};

/// A MBR Partition Entry (16 bytes)
#[derive(Debug, Default, Clone)]
pub struct MBRPartitionEntry {
    pub boot_indicator: u8, // Bootable (0x80 for active, 0x00 for inactive)
    pub start_chs: [u8; 3], // CHS (Cylinder-Head-Sector) address of the start of the partition
    pub partition_type: u8, // Partition type (e.g., 0x07 for NTFS, 0x83 for Linux)
    pub end_chs: [u8; 3],   // CHS address of the end of the partition
    pub start_lba: u32,     // Start address of the partition in LBA (Logical Block Addressing)
    pub size_sectors: u32,  // Size of the partition in sectors
    pub sector_size: usize, // Size of a sector
    pub first_byte_addr: usize,
}

impl MBRPartitionEntry {
    /// Returns a human-readable description of the partition type.
    /// Ref: https://en.wikipedia.org/wiki/Partition_type
    pub fn partition_type_description(&self) -> &str {
        match self.partition_type {
            0x00 => "Unused",
            0x01 => "FAT12",
            0x02 => "XENIX root",
            0x03 => "XENIX usr",
            0x04 => "FAT16 <32M",
            0x05 => "Extended",
            0x06 => "FAT16B",
            0x07 => "NTFS/exFAT/IFS/HPFS",
            0x08 => "AIX boot/split/QNX",
            0x09 => "AIX data/boot/QNX",
            0x0A => "OS/2 Boot Manager/Coherent Swap",
            0x0B => "W95 FAT32",
            0x0C => "W95 FAT32 (LBA)",
            0x0E => "W95 FAT16 (LBA)",
            0x0F => "W95 Ext'd (LBA)",
            0x10 => "Reserved",
            0x11 => "Hidden FAT12",
            0x12 => "Hibernation/Service FS/Rescue & Recovery",
            0x14 => "Logical FAT12/FAT16/Hidden FAT16/Omega",
            0x15 => "Hidden Extended/Swap",
            0x16 => "Hidden FAT16B",
            0x17 => "Hidden IFS/HPFS/NTFS/exFAT",
            0x18 => "AST Zero Volt Suspend/SmartSleep",
            0x19 => "Willowtech Photon coS",
            0x1B => "Hidden FAT32",
            0x1C => "Hidden FAT32 with LBA/ASUS eRecovery",
            0x1E => "Hidden FAT16 with LBA",
            0x20 => "Windows Mobile Update XIP/Willowsoft OFS1",
            0x21 => "HP Volume Expansion",
            0x22 => "Oxygen Extended Partition Table",
            0x23 => "Windows Mobile boot XIP",
            0x27 => "Windows Recovery Environment/RooterBOOT",
            0x2A => "AtheOS ForthOS",
            0x2B => "SyllableSecure (SylStor)",
            0x31 => "Reserved",
            0x33 => "Reserved",
            0x34 => "Reserved",
            0x36 => "Reserved",
            0x38 => "THEOS v3.2",
            0x39 => "Plan 9/TheOS v4 spanned",
            0x3A => "THEOS v4 (4 GB)",
            0x3B => "THEOS v4 extended",
            0x3C => "PartitionMagic PqRP",
            0x3D => "PartitionMagic Hidden NetWare",
            0x40 => "PICK Systems/Venix/Venix 80286",
            0x41 => "Personal RISC Boot/Linux/PowerPC",
            0x42 => "Secure File System/Old Linux Swap/Dynamic Extended",
            0x43 => "Old Linux Native",
            0x44 => "Wildfile GoBack",
            0x45 => "Priam/EUMEL/ELAN",
            0x46 => "EUMEL/ELAN",
            0x47 => "EUMEL/ELAN",
            0x48 => "EUMEL/ELAN and ERGOS L3",
            0x4C => "Aos (A2) File System",
            0x4D => "Primary QNX POSIX",
            0x4E => "Secondary QNX POSIX",
            0x4F => "Tertiary QNX POSIX",
            0x50 => "Alternative Native/LynxOS/Novell Operations",
            0x51 => "Novell Read-Write/Kurt Skauen Toggle",
            0x52 => "System V/AT, V/386",
            0x53 => "Disk Manager 6 Auxiliary 3",
            0x54 => "Disk Manager 6 Dynamic Drive Overlay",
            0x55 => "EZ-Drive INT 13h Redirector Volume",
            0x56 => "Logical FAT12/FAT16/EZ-BIOS/VFeature",
            0x57 => "DrivePro/VNDI",
            0x5C => "Priam EDisk",
            0x63 => "SCO Unix/ISC/UnixWare/BSD",
            0x64 => "SpeedStor Hidden FAT16/Novell NetWare/PC-ARMOUR",
            0x65 => "Novell NetWare File System 386",
            0x66 => "Novell Storage Management Services",
            0x67 => "Novell Wolf Mountain Cluster",
            0x68 => "Reserved for DR-DOS",
            0x69 => "Novell NSS/NetWare 5",
            0x70 => "DiskSecure Multiboot",
            0x71 => "Reserved",
            0x72 => "Unix V7/x86",
            0x73 => "Reserved",
            0x75 => "IBM PC/IX",
            0x76 => "SpeedStor Hidden read-only FAT16B",
            0x77 => "Novell VNDI/M2FS/M2CS",
            0x78 => "XOSL Bootloader",
            0x80 => "Minix 1.1-1.4a",
            0x81 => "Minix 1.4b+",
            0x82 => "Linux Swap",
            0x83 => "Linux/GNU Hurd",
            0x84 => "APM Hibernation/Hidden FAT16",
            0x85 => "Linux Extended",
            0x86 => "Microsoft Fault-tolerant FAT16B mirrored",
            0x87 => "Microsoft Fault-tolerant HPFS/NTFS mirrored",
            0x88 => "Linux Plaintext Partition Table",
            0x8A => "AirBoot Boot Manager",
            0x8B => "Legacy FAT32 Mirrored (0Bh)",
            0x8C => "Legacy FAT32 Mirrored (0Ch)",
            0x8E => "Linux LVM",
            0x93 => "Amoeba/Amoeba Native",
            0x94 => "Amoeba Bad Block Table",
            0x95 => "EXOPC Native",
            0x96 => "ISO-9660",
            0x99 => "Early Unix",
            0x9E => "VSTa/ForthOS",
            0x9F => "BSD/OS 3.0+",
            0xA0 => "Hibernate Partition",
            0xA1 => "HP Volume Expansion/Hibernate Partition",
            0xA3 => "HP Volume Expansion",
            0xA4 => "HP Volume Expansion",
            0xA6 => "HP Volume Expansion",
            0xA7 => "NeXTSTEP",
            0xA8 => "Apple Darwin/Mac OS X UFS",
            0xAB => "Apple Darwin/Mac OS X Boot",
            0xAC => "Apple RAID",
            0xAD => "RISC OS FileCore",
            0xAE => "ShagOS File System",
            0xAF => "Mac OS X HFS",
            0xB0 => "Boot-Star Dummy Partition",
            0xB1 => "HP Volume Expansion/QNX Neutrino",
            0xB2 => "QNX Neutrino Power-safe File System",
            0xB3 => "HP Volume Expansion/QNX Neutrino",
            0xB4 => "HP Volume Expansion",
            0xB6 => "Corrupted FAT16B Mirrored Master",
            0xB7 => "Corrupted HPFS/NTFS Mirrored Master",
            0xB8 => "BSDI Swap/Native",
            0xBB => "Acronis True Image OEM Secure Zone",
            0xBD => "BonnyDOS/286",
            0xBE => "Solaris 8 Boot",
            0xBF => "Solaris x86",
            0xC1 => "DR DOS 6.0+ Secured FAT12",
            0xC2 => "Power Boot Hidden FS",
            0xC3 => "Power Boot Hidden Swap",
            0xC4 => "DR DOS 6.0+ Secured FAT16",
            0xC5 => "DR DOS 6.0+ Secured Extended",
            0xC6 => "DR DOS 6.0+ Secured FAT16B",
            0xCB => "Caldera DR-DOS 7.0x Secured FAT32",
            0xCC => "Caldera DR-DOS 7.0x Secured FAT32",
            0xCE => "Caldera DR-DOS 7.0x Secured FAT16B",
            0xCF => "Caldera DR-DOS 7.0x Secured Extended",
            0xD1 => "Novell Multiuser DOS Secured FAT12",
            0xD4 => "Novell Multiuser DOS Secured FAT16",
            0xD5 => "Novell Multiuser DOS Secured Extended",
            0xD6 => "Novell Multiuser DOS Secured FAT16B",
            0xD8 => "CP/M-86",
            0xDB => "CP/M-86/Concurrent DOS/FAT32 System Restore",
            0xDE => "Dell FAT16 Utility/Diagnostic",
            0xDF => "BootIt",
            0xE0 => "ST AVFS",
            0xE1 => "SpeedStor FAT12",
            0xE3 => "SpeedStor Read-only FAT12",
            0xE4 => "SpeedStor FAT16",
            0xE5 => "Logical FAT12/FAT16",
            0xE6 => "SpeedStor Read-only FAT16",
            0xEB => "BFS (BeOS/Haiku)",
            0xEC => "SkyOS SkyFS",
            0xF0 => "PA-RISC Linux Boot Loader",
            0xF2 => "Secondary FAT12",
            0xF4 => "SpeedStor FAT16B",
            0xF5 => "Prologue MD0-MD9",
            0xF7 => "EFAT/DDRdrive Solid State FS",
            0xF9 => "pCache ext2/ext3",
            0xFB => "VMware VMFS",
            0xFC => "VMware Swap/VMKCORE",
            0xFD => "Linux RAID superblock",
            0xFE => "PS/2 IML/Old Linux LVM",
            0xFF => "XENIX bad block table",
            _ => "Unknown",
        }
    }

    /// Converts raw CHS bytes into a human-readable tuple
    fn chs_tuple(bytes: [u8; 3]) -> (u16, u8, u8) {
        // Extract Head from the first byte
        let head = bytes[0];
        // Extract Sector from the lower 6 bits of the second byte
        let sector = bytes[1] & 0x3F;
        // Extract the upper 2 bits of the second byte and combine with the third byte to get Cylinder
        let cylinder = ((bytes[1] as u16 & 0xC0) << 2) | (bytes[2] as u16);
        (cylinder, head, sector)
    }

    /// Get readable CHS start tuple
    pub fn start_chs_tuple(&self) -> (u16, u8, u8) {
        MBRPartitionEntry::chs_tuple(self.start_chs)
    }

    /// Get readable CHS end tuple
    pub fn end_chs_tuple(&self) -> (u16, u8, u8) {
        MBRPartitionEntry::chs_tuple(self.end_chs)
    }

    pub fn get_first_byte_address(&self) -> usize {
        self.sector_size * self.start_lba as usize
    }
}

/// MBR Structure (512 bytes)
#[derive(Debug)]
pub struct MBR {
    pub bootloader: [u8; 446], // Bootloader code (size: 446 bytes)
    pub partition_table: [MBRPartitionEntry; 4], // Partition table (max 4 entries)
    pub boot_signature: u16,   // the value should be 0x55AA
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
                sector_size: 512, // The default value but it might be changed
                first_byte_addr: 0,
            };
            mbr.partition_table[i].first_byte_addr =
                mbr.partition_table[i].sector_size * mbr.partition_table[i].start_lba as usize;
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
            Cell::new("Start address (LBA)"),
            Cell::new("Partition type"),
            Cell::new("Type Description"),
            Cell::new("First byte address"),
            Cell::new("Size (in sectors)"),
        ]));

        for partition in &self.partition_table {
            partitions_table.add_row(Row::new(vec![
                Cell::new(&(format!("{:?}", partition.boot_indicator))),
                Cell::new(&(format!("{:?}", partition.start_chs_tuple()))),
                Cell::new(&(format!("{:?}", partition.end_chs_tuple()))),
                Cell::new(&(format!("0x{:x}", partition.start_lba))),
                Cell::new(&(format!("0x{:02x}", partition.partition_type))),
                Cell::new(partition.partition_type_description()),
                Cell::new(&(format!("0x{:x}", partition.first_byte_addr))),
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
