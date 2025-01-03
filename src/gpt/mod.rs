use byteorder::{LittleEndian, ReadBytesExt};
use std::io::{Cursor, Read};

/// GPT Header (92 bytes)
#[derive(Debug, Default)]
pub struct GPTHeader {
    pub signature: [u8; 8],     // GPT signature ("EFI PART" in ASCII)
    revision: u32,              // GPT revision (typically 0x00010000)
    header_size: u32,           // Size of the GPT header (typically 92 bytes)
    crc32: u32,                 // CRC32 checksum of the GPT header
    reserved: u32,              // Reserved (usually 0)
    my_lba: u64,                // LBA of the GPT header (usually 1)
    backup_lba: u64,            // LBA of the backup GPT header (typically last sector of the disk)
    first_usable_lba: u64,      // LBA of the first usable partition (typically 34)
    last_usable_lba: u64,       // LBA of the last usable partition
    disk_guid: [u8; 16],        // Unique disk GUID
    partition_entry_lba: u64,   // LBA of the partition entry array
    num_partition_entries: u32, // Number of partition entries (typically 128)
    partition_entry_size: u32,  // Size of each partition entry (typically 128 bytes)
    partition_array_crc32: u32, // CRC32 checksum of the partition entry array
}

/// GPT Partition Entry (128 bytes)
#[derive(Debug)]
pub struct GPTPartitionEntry {
    partition_guid: [u8; 16],      // GUID of the partition
    partition_type_guid: [u8; 16], // GUID of the partition type (e.g., Linux, Windows)
    starting_lba: u64,             // Starting LBA of the partition
    ending_lba: u64,               // Ending LBA of the partition
    attributes: u64,               // Partition attributes (e.g., hidden, read-only)
    partition_name: [u16; 36],     // Partition name (UTF-16)
}

/// GPT Structure (contains header and partition entries)
#[derive(Debug, Default)]
pub struct GPT {
    pub header: GPTHeader,                     // GPT header
    partition_entries: Vec<GPTPartitionEntry>, // Partition entries
}

impl GPT {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut cursor = Cursor::new(bytes);
        let mut gpt = GPT::default();

        // Read GPT Header (92 bytes)
        gpt.header.signature.copy_from_slice(
            &cursor
                .clone()
                .take(8)
                .bytes()
                .map(|b| b.unwrap())
                .collect::<Vec<u8>>(),
        );
        gpt.header.revision = cursor.read_u32::<LittleEndian>().unwrap();
        gpt.header.header_size = cursor.read_u32::<LittleEndian>().unwrap();
        gpt.header.crc32 = cursor.read_u32::<LittleEndian>().unwrap();
        gpt.header.reserved = cursor.read_u32::<LittleEndian>().unwrap();
        gpt.header.my_lba = cursor.read_u64::<LittleEndian>().unwrap();
        gpt.header.backup_lba = cursor.read_u64::<LittleEndian>().unwrap();
        gpt.header.first_usable_lba = cursor.read_u64::<LittleEndian>().unwrap();
        gpt.header.last_usable_lba = cursor.read_u64::<LittleEndian>().unwrap();
        cursor.read_exact(&mut gpt.header.disk_guid).unwrap();
        gpt.header.partition_entry_lba = cursor.read_u64::<LittleEndian>().unwrap();
        gpt.header.num_partition_entries = cursor.read_u32::<LittleEndian>().unwrap();
        gpt.header.partition_entry_size = cursor.read_u32::<LittleEndian>().unwrap();
        gpt.header.partition_array_crc32 = cursor.read_u32::<LittleEndian>().unwrap();

        // Read Partition Entries (128 bytes each)
        let num_entries = gpt.header.num_partition_entries as usize;
        gpt.partition_entries = Vec::with_capacity(num_entries);

        for _ in 0..num_entries {
            let mut entry = GPTPartitionEntry {
                partition_guid: [0u8; 16],
                partition_type_guid: [0u8; 16],
                starting_lba: 0u64,
                ending_lba: 0u64,
                attributes: 0u64,
                partition_name: [0u16; 36],
            };
            cursor.read_exact(&mut entry.partition_guid).unwrap();
            cursor.read_exact(&mut entry.partition_type_guid).unwrap();
            entry.starting_lba = cursor.read_u64::<LittleEndian>().unwrap();
            entry.ending_lba = cursor.read_u64::<LittleEndian>().unwrap();
            entry.attributes = cursor.read_u64::<LittleEndian>().unwrap();

            let mut buffer = [0u8; 72]; // 36 * 2 bytes = 72 bytes
            cursor.read_exact(&mut buffer).unwrap();
            entry.partition_name = buffer
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect::<Vec<_>>()
                .try_into()
                .unwrap();
            gpt.partition_entries.push(entry);
        }

        gpt
    }
}
