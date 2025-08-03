use byteorder::{LittleEndian, ReadBytesExt};
use prettytable::{Cell, Row, Table};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read};

/// GPT Header (92 bytes)
#[derive(Serialize, Default, Deserialize, Debug, Clone)]
pub struct GPTHeader {
    pub signature: [u8; 8],    // GPT signature ("EFI PART" in ASCII)
    pub revision: u32,         // GPT revision (typically 0x00010000)
    pub header_size: u32,      // Size of the GPT header (typically 92 bytes)
    pub crc32: u32,            // CRC32 checksum of the GPT header
    pub reserved: u32,         // Reserved (usually 0)
    pub current_lba: u64,      // LBA of the GPT header (usually 1)
    pub backup_lba: u64,       // LBA of the backup GPT header (typically last sector of the disk)
    pub first_usable_lba: u64, // LBA of the first usable partition (typically 34)
    pub last_usable_lba: u64,  // LBA of the last usable partition
    pub disk_guid: [u8; 16],   // Unique disk GUID
    pub disk_guid_string: String,
    pub partition_entry_lba: u64,   // LBA of the partition entry array
    pub num_partition_entries: u32, // Number of partition entries (typically 128)
    pub partition_entry_size: u32,  // Size of each partition entry (typically 128 bytes)
    pub partition_array_crc32: u32, // CRC32 checksum of the partition entry array
}

/// GPT Partition Entry (128 bytes)
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct GPTPartitionEntry {
    pub id: Option<i64>, // Optionnal but used by Thanatology: Give an unique ID to the
    pub partition_guid: [u8; 16], // GUID of the partition
    pub partition_guid_string: String, // String GUID of the partition
    pub partition_type_guid: [u8; 16], // GUID of the partition type (e.g., Linux, Windows)
    pub partition_type_guid_string: String, // GUID String of the partition type.
    pub description: String, // Partition description string
    pub starting_lba: u64, // Starting LBA of the partition
    pub first_byte_addr: u64, // Absolute address
    pub size_sectors: u64, // Size (in sectors)
    pub ending_lba: u64, // Ending LBA of the partition
    pub attributes: u64, // Partition attributes (e.g., hidden, read-only)
    pub partition_name: String, // Partition name (UTF-16)
}

/// GPT Structure (contains header and partition entries)
#[derive(Serialize, Default, Deserialize, Debug, Clone)]
pub struct GPT {
    pub header: GPTHeader,                         // GPT header
    pub partition_entries: Vec<GPTPartitionEntry>, // Partition entries
}

impl GPTPartitionEntry {
    pub fn partition_type_description(&self) -> &str {
        // Convert our 16-byte GUID into a canonical string
        let guid = format_guid(&self.partition_type_guid);
        match guid.as_str() {
            "00000000-0000-0000-0000-000000000000" => "Unused entry",

            // MBR partition scheme
            "024dee41-33e7-11d3-9d69-0008c781f39f" => "MBR partition scheme",

            // EFI/UEFI
            "c12a7328-f81f-11d2-ba4b-00a0c93ec93b" => "EFI System partition",

            // BIOS boot
            "21686148-6449-6e6f-744e-656564454649" => "BIOS boot partition",

            // Intel Rapid Start (iFFS)
            "d3bfe2de-3daf-11df-ba40-e3a556d89593" => "Intel Fast Flash (iFFS) partition",

            // Sony
            "f4019732-066e-4e12-8273-346c5641494f" => "Sony boot partition",

            // Lenovo
            "bfbfafe7-a34f-448a-9a5b-6213eb736c22" => "Lenovo boot partition",

            // Windows
            "e3c9e316-0b5c-4db8-817d-f92df00215ae" => "Microsoft Reserved Partition (MSR)",
            "ebd0a0a2-b9e5-4433-87c0-68b6b72699c7" => "Basic data partition",
            "5808c8aa-7e8f-42e0-85d2-e1e90434cfb3" => {
                "Logical Disk Manager (LDM) metadata partition"
            }
            "af9b60a0-1431-4f62-bc68-3311714a69ad" => "Logical Disk Manager data partition",
            "de94bba4-06d1-4d40-a16a-bfd50179d6ac" => "Windows Recovery Environment",

            // IBM GPFS
            "37affc90-ef7d-4e96-91c3-2d7ae055b174" => {
                "IBM General Parallel File System (GPFS) partition"
            }

            // Storage Spaces/Replica
            "e75caf8f-f680-4cee-afa3-b001e56efc2d" => "Storage Spaces partition",
            "558d43c5-a1ac-43c0-aac8-d1472b2923d1" => "Storage Replica partition",

            // HP-UX
            "75894c1e-3aeb-11d3-b7c1-7b03a0000000" => "HP-UX Data partition",
            "e2a1e728-32e3-11d6-a682-7b03a0000000" => "Service partition",

            // Linux
            "0fc63daf-8483-4772-8e79-3d69d8477de4" => "Linux filesystem data",
            "a19d880f-05fc-4d3b-a006-743f0f84911e" => "RAID partition",

            // Linux platform-specific partitions
            "6523f8ae-3eb1-4e2a-a05a-18b695ae656f" => "Root partition (Alpha)",
            "d27f46ed-2919-4cb8-bd25-9531f3c16534" => "ARC",
            "69dad710-2ce4-4e3c-b16c-21a1d49abed3" => "ARM 32‑bit",
            "b921b045-1df0-41c3-af44-4c6f280d3fae" => "AArch64",
            "993d8d3d-f80e-4225-855a-9daf8ed7ea97" => "IA-64",
            "77055800-792c-4f94-b39a-98c91b762bb6" => "LoongArch 64‑bit",
            "e9434544-6e2c-47cc-bae2-12d6deafb44c" => "32‑bit MIPS (big‑endian)",
            "d113af76-80ef-41b4-bdb6-0cff4d3d4a25" => "64‑bit MIPS (big‑endian)",
            "37c58c8a-d913-4156-a25f-48b1b64e07f0" => "32‑bit MIPS (little‑endian)",
            "700bda43-7a34-4507-b179-eeb93d7a7ca3" => "64‑bit MIPS (little‑endian)",
            "1aacdb3b-5444-4138-bd9e-e5c2239b2346" => "PA‑RISC",
            "1de3f1ef-fa98-47b5-8dcd-4a860a654d78" => "32‑bit PowerPC",
            "912ade1d-a839-4913-8964-a10eee08fbd2" => "64‑bit PowerPC (big‑endian)",
            "c31c45e6-3f39-412e-80fb-4809c4980599" => "64‑bit PowerPC (little‑endian)",
            "60d5a7fe-8e7d-435c-b714-3dd8162144e1" => "RISC‑V 32‑bit",
            "72ec70a6-cf74-40e6-bd49-4bda08e8f224" => "RISC‑V 64‑bit",
            "08a7acea-624c-4a20-91e8-6e0fa67d23f9" => "s390",
            "5eead9a9-fe09-4a1e-a1d7-520d00531306" => "s390x",
            "c50cdd70-3862-4cc3-90e1-809a8c93ee2c" => "TILE‑Gx",
            "44479540-f297-41b2-9af7-d131d5f0458a" => "x86",
            "4f68bce3-e8cd-4db1-96e7-fbcaf984b709" => "x86‑64",

            // /usr and related partitions (multiple variants)
            "e18cf08c-33ec-4c0d-8246-c6c6fb3da024" => "/usr partition (Alpha)",
            "7978a683-6316-4922-bbee-38bff5a2fecc" => "ARC (Alpha)",
            "7d0359a3-02b3-4f0a-865c-654403e70625" => "ARM 32‑bit (Alpha)",
            "b0e01050-ee5f-4390-949a-9101b17104e9" => "AArch64 (Alpha)",
            "4301d2a6-4e3b-4b2a-bb94-9e0b2c4225ea" => "IA‑64 (Alpha)",
            "e611c702-575c-4cbe-9a46-434fa0bf7e3f" => "LoongArch 64‑bit (Alpha)",
            "773b2abc-2a99-4398-8bf5-03baac40d02b" => "32‑bit MIPS (big‑endian, Alpha)",
            "57e13958-7331-4365-8e6e-35eeee17c61b" => "64‑bit MIPS (big‑endian, Alpha)",
            "0f4868e9-9952-4706-979f-3ed3a473e947" => "32‑bit MIPS (little‑endian, Alpha)",
            "c97c1f32-ba06-40b4-9f22-236061b08aa8" => "64‑bit MIPS (little‑endian, Alpha)",
            "dc4a4480-6917-4262-a4ec-db9384949f25" => "PA‑RISC (Alpha)",
            "7d14fec5-cc71-415d-9d6c-06bf0b3c3eaf" => "32‑bit PowerPC (Alpha)",
            "2c9739e2-f068-46b3-9fd0-01c5a9afbcca" => "64‑bit PowerPC (big‑endian, Alpha)",
            "15bb03af-77e7-4d4a-b12b-c0d084f7491c" => "64‑bit PowerPC (little‑endian, Alpha)",
            "b933fb22-5c3f-4f91-af90-e2bb0fa50702" => "RISC‑V 32‑bit (Alpha)",
            "beaec34b-8442-439b-a40b-984381ed097d" => "RISC‑V 64‑bit (Alpha)",
            "cd0f869b-d0fb-4ca0-b141-9ea87cc78d66" => "s390 (Alpha)",
            "8a4f5770-50aa-4ed3-874a-99b710db6fea" => "s390x (Alpha)",
            "55497029-c7c1-44cc-aa39-815ed1558630" => "TILE‑Gx (Alpha)",
            "75250d76-8cc6-458e-bd66-bd47cc81a812" => "x86 (Alpha)",
            "8484680c-9521-48c6-9c11-b0720656f69e" => "x86‑64 (Alpha)",

            // (Additional groups – dm‑verity, /usr verity, signature partitions, etc.)
            // For example, the dm‑verity group:
            "fc56d9e9-e6e5-4c06-be32-e74407ce09a5" => "Root verity partition for dm‑verity (Alpha)",
            "24b2d975-0f97-4521-afa1-cd531e421b8d" => "ARC (dm‑verity)",
            "7386cdf2-203c-47a9-a498-f2ecee45a2d6" => "ARM 32‑bit (dm‑verity)",
            "df3300ce-d69f-4c92-978c-9bfb0f38d820" => "AArch64 (dm‑verity)",
            "86ed10d5-b607-45bb-8957-d350f23d0571" => "IA‑64 (dm‑verity)",
            "f3393b22-e9af-4613-a948-9d3bfbd0c535" => "LoongArch 64‑bit (dm‑verity)",
            "7a430799-f711-4c7e-8e5b-1d685bd48607" => "32‑bit MIPS (dm‑verity)",
            "579536f8-6a33-4055-a95a-df2d5e2c42a8" => "64‑bit MIPS (dm‑verity)",
            "d7d150d2-2a04-4a33-8f12-16651205ff7b" => "32‑bit MIPS (dm‑verity, little‑endian)",
            "16b417f8-3e06-4f57-8dd2-9b5232f41aa6" => "64‑bit MIPS (dm‑verity, little‑endian)",
            "d212a430-fbc5-49f9-a983-a7feef2b8d0e" => "PA‑RISC (dm‑verity)",
            "906bd944-4589-4aae-a4e4-dd983917446a" => "64‑bit PowerPC (dm‑verity, little‑endian)",
            "9225a9a3-3c19-4d89-b4f6-eeff88f17631" => "64‑bit PowerPC (dm‑verity, big‑endian)",
            "98cfe649-1588-46dc-b2f0-add147424925" => "32‑bit PowerPC (dm‑verity)",
            "ae0253be-1167-4007-ac68-43926c14c5de" => "RISC‑V 32‑bit (dm‑verity)",
            "b6ed5582-440b-4209-b8da-5ff7c419ea3d" => "RISC‑V 64‑bit (dm‑verity)",
            "7ac63b47-b25c-463b-8df8-b4a94e6c90e1" => "s390 (dm‑verity)",
            "b325bfbe-c7be-4ab8-8357-139e652d2f6b" => "s390x (dm‑verity)",
            "966061ec-28e4-4b2e-b4a5-1f0a825a1d84" => "TILE‑Gx (dm‑verity)",
            "2c7357ed-ebd2-46d9-aec1-23d437ec2bf5" => "x86‑64 (dm‑verity)",
            "d13c5d3b-b5d1-422a-b29f-9454fdc89d76" => "x86 (dm‑verity)",
            "48465300-0000-11aa-aa11-00306543ecac" => "Apple HFS+",
            // (The rest of the table follows in the same pattern:)
            // – /boot (XBOOTLDR)
            "bc13c2ff-59e6-4262-a352-b275fd6f7172" => "/boot (XBOOTLDR) partition",
            // – Swap partitions
            "0657fd6d-a4ab-43c4-84e5-0933c84b4f4f" => "Swap partition",
            // – LVM, /home, /srv, per‑user home, dm‑crypt, LUKS, Reserved, GNU/Hurd, FreeBSD, BSD disklabel, UFS, Vinum, ZFS, nandfs,
            // LVM
            "e6d1d9b7-95b3-4a3d-b114-85ff3d230a6e" => "LVM partition",
            // /home
            "933ac7e1-2eb4-4f13-b844-0e14e2aef915" => "/home partition",
            // /srv
            "3b8f8425-20e0-4f3b-907f-1a25a76f98e8" => "/srv partition",
            // per‑user home
            "69646981-091c-4e43-9c84-b7b35b13c7e6" => "Per-user home partition",
            // dm‑crypt
            "7ffec5c9-2d00-49b1-988a-c22c947ffee7" => "Encrypted partition (dm-crypt)",
            // LUKS
            "ca7d7ccb-63ed-4c53-bb4a-2e387187f96d" => "LUKS partition",
            // macOS
            "48465300-0000-11AA-AA11-00306543ECAC" => "HFS+ partition",
            "7c3457ef-0000-11aa-aa11-00306543ecac" => "APFS partition",
            // GNU/Hurd
            "3bd3c9df-5f3c-4b0b-9d22-5d1b012fcf10" => "GNU/Hurd root partition",
            // FreeBSD
            "516e7cb4-6ecf-11d6-8ff8-00022d09712b" => "FreeBSD data partition",
            // Solaris
            "6a82cb45-1dd2-11b2-99a6-080020736631" => "Solaris Boot partition",
            // Ceph
            "4fbd7e29-9d25-41b8-afd0-062c0ceff05d" => "Ceph OSD partition",
            // Android-IA
            "e6a0c4fe-1339-466b-9aef-ef9e2ab8fa56" => "Android-IA AVB",
            _ => "Unknown partition type",
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut cur = Cursor::new(&bytes);
        let mut entry = GPTPartitionEntry::default();
        cur.read_exact(&mut entry.partition_type_guid)
            .expect("Could not read the partition type GUID.");
        cur.read_exact(&mut entry.partition_guid)
            .expect("Could not read the partition GUID.");
        entry.starting_lba = cur
            .read_u64::<LittleEndian>()
            .expect("Could not read the starting LBA.");
        entry.ending_lba = cur
            .read_u64::<LittleEndian>()
            .expect("Could not read the ending LBA.");
        entry.attributes = cur
            .read_u64::<LittleEndian>()
            .expect("Could not read the partition attributes");

        let mut utf16 = vec![0u16; 36];
        cur.read_u16_into::<LittleEndian>(&mut utf16)
            .expect("Could not read the partition name");
        entry.partition_name = String::from_utf16_lossy(&utf16);
        entry.description = entry.partition_type_description().to_string();
        entry.partition_type_guid_string = format_guid(&entry.partition_type_guid);
        entry.partition_guid_string = format_guid(&entry.partition_guid);
        entry
    }
}

impl GPT {
    pub fn is_gpt(&self) -> bool {
        self.header.signature == *b"EFI PART"
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut cursor = Cursor::new(bytes);
        let mut gpt = GPT::default();

        // Read GPT Header (92 bytes)
        cursor.read_exact(&mut gpt.header.signature).unwrap();
        gpt.header.revision = cursor.read_u32::<LittleEndian>().unwrap();
        gpt.header.header_size = cursor.read_u32::<LittleEndian>().unwrap();
        gpt.header.crc32 = cursor.read_u32::<LittleEndian>().unwrap();
        gpt.header.reserved = cursor.read_u32::<LittleEndian>().unwrap();
        gpt.header.current_lba = cursor.read_u64::<LittleEndian>().unwrap();
        gpt.header.backup_lba = cursor.read_u64::<LittleEndian>().unwrap();
        gpt.header.first_usable_lba = cursor.read_u64::<LittleEndian>().unwrap();
        gpt.header.last_usable_lba = cursor.read_u64::<LittleEndian>().unwrap();
        cursor.read_exact(&mut gpt.header.disk_guid).unwrap();
        gpt.header.disk_guid_string = format_guid(&mut gpt.header.disk_guid);
        gpt.header.partition_entry_lba = cursor.read_u64::<LittleEndian>().unwrap();
        gpt.header.num_partition_entries = cursor.read_u32::<LittleEndian>().unwrap();
        gpt.header.partition_entry_size = cursor.read_u32::<LittleEndian>().unwrap();
        gpt.header.partition_array_crc32 = cursor.read_u32::<LittleEndian>().unwrap();

        gpt
    }

    pub fn print_info(&self) -> String {
        let mut gpt_table = Table::new();
        let mut partitions_table = Table::new();
        gpt_table.add_row(Row::new(vec![
            Cell::new("Signature"),
            Cell::new(&format!(
                "{}",
                String::from_utf8_lossy(&self.header.signature)
            )),
        ]));

        gpt_table.add_row(Row::new(vec![
            Cell::new("Revision"),
            Cell::new(&format!("0x{:x}", &self.header.revision)),
        ]));

        gpt_table.add_row(Row::new(vec![
            Cell::new("Header size"),
            Cell::new(&format!("0x{:x}", &self.header.header_size)),
        ]));
        gpt_table.add_row(Row::new(vec![
            Cell::new("CRC32"),
            Cell::new(&format!("0x{:x}", &self.header.crc32)),
        ]));
        gpt_table.add_row(Row::new(vec![
            Cell::new("Current LBA"),
            Cell::new(&format!("0x{:x}", &self.header.current_lba)),
        ]));
        gpt_table.add_row(Row::new(vec![
            Cell::new("Backup LBA"),
            Cell::new(&format!("0x{:x}", &self.header.backup_lba)),
        ]));
        gpt_table.add_row(Row::new(vec![
            Cell::new("First Usable LBA"),
            Cell::new(&format!("0x{:x}", &self.header.first_usable_lba)),
        ]));
        gpt_table.add_row(Row::new(vec![
            Cell::new("Disk GUID"),
            Cell::new(&format!("{}", &self.header.disk_guid_string)),
        ]));
        gpt_table.add_row(Row::new(vec![
            Cell::new("Partition Entry LBA"),
            Cell::new(&format!("0x{:x}", &self.header.partition_entry_lba)),
        ]));
        gpt_table.add_row(Row::new(vec![
            Cell::new("Number Of Entries"),
            Cell::new(&format!("0x{:x}", &self.header.num_partition_entries)),
        ]));
        gpt_table.add_row(Row::new(vec![
            Cell::new("Size of one Entry "),
            Cell::new(&format!("0x{:x}", &self.header.partition_entry_size)),
        ]));
        gpt_table.add_row(Row::new(vec![
            Cell::new("Partition Array CRC32"),
            Cell::new(&format!("0x{:x}", &self.header.partition_array_crc32)),
        ]));

        partitions_table.add_row(Row::new(vec![
            Cell::new("GUID"),
            Cell::new("Type GUID"),
            Cell::new("Description"),
            Cell::new("Start addr (LBA)"),
            Cell::new("End addr (LBA)"),
            Cell::new("Start addr (Absolute)"),
            Cell::new("Size (sectors)"),
            Cell::new("Attributes"),
            Cell::new("Partition Name"),
        ]));
        for partition in &self.partition_entries {
            // Let's not display unused entries 00000000-0000-0000-0000-000000000000
            if partition.partition_type_guid != [0u8; 16] {
                partitions_table.add_row(Row::new(vec![
                    Cell::new(&format!("{}", &partition.partition_guid_string)),
                    Cell::new(&format!("{}", &partition.partition_type_guid_string)),
                    Cell::new(&format!("{}", partition.description)),
                    Cell::new(&format!("0x{:x}", partition.starting_lba)),
                    Cell::new(&format!("0x{:x}", partition.ending_lba)),
                    Cell::new(&format!("0x{:x}", partition.first_byte_addr)),
                    Cell::new(&format!("0x{:x}", partition.size_sectors)),
                    Cell::new(&format!("{:?}", partition.attributes)),
                    Cell::new(&format!("{}", partition.partition_name)),
                ]));
            }
        }
        gpt_table.add_row(Row::new(vec![
            Cell::new("Partition tables entries"),
            Cell::new(&partitions_table.to_string()),
        ]));
        gpt_table.to_string()
    }
}

pub fn format_guid(guid: &[u8; 16]) -> String {
    format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            guid[3], guid[2], guid[1], guid[0],
            guid[5], guid[4],
            guid[7], guid[6],
            guid[8], guid[9],
            guid[10], guid[11], guid[12], guid[13], guid[14], guid[15]
        )
}
