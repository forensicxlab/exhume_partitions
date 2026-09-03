pub mod ebr;
pub mod extent;
pub mod gpt;
pub mod mbr;

pub use extent::{
    GptPartitionSelector, PartitionExtent, PartitionExtentError, PartitionScheme, VolumeExtent,
};
pub use gpt::GptParseError;

use exhume_body::Body;
use gpt::{GPTHeader, GPTPartitionEntry, GPT, GPT_HEADER_MIN_SIZE, GPT_PARTITION_ENTRY_MIN_SIZE};
use log::{info, warn};
use mbr::MBR;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::error::Error;
use std::io::{Read, Seek, SeekFrom};

const MAX_GPT_PARTITION_ENTRIES: u32 = 16_384;
const MAX_GPT_PARTITION_ENTRY_SIZE: u32 = 4_096;
const MAX_GPT_PARTITION_ARRAY_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
struct GptTableGeometry {
    offset_bytes: u64,
    length_bytes: u64,
}

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

        let ebr_record = mbr_record
            .as_ref()
            .map(|mbr| discover_ebr_partitions(body, mbr));

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

        if let Some(mbr) = &self.mbr {
            s.push_str(&mbr.print_info(&bootloader));
        }
        s.push('\n');

        if let Some(ebr) = &self.ebr {
            if !ebr.is_empty() {
                for ebr_entry in ebr {
                    s.push_str(&ebr_entry.print_info(&bootloader))
                }
            }
        }

        if let Some(gpt) = &self.gpt {
            s.push_str(&gpt.print_info());
        }

        s
    }
}

fn discover_any_gpt(body: &mut Body) -> Result<GPT, Box<dyn Error>> {
    discover_gpt_partitions(body, false) // primary @ LBA 1
        .or_else(|_| discover_gpt_partitions(body, true)) // backup @ last LBA
}

/// Discover a CRC-validated GPT without parsing MBR extended-partition chains.
///
/// Consumers that explicitly select a GPT extent should use this fallible,
/// bounded path rather than [`Partitions::new`], which also inventories legacy
/// MBR/EBR structures for its broader reporting API.
pub fn discover_gpt(body: &mut Body) -> Result<GPT, Box<dyn Error>> {
    discover_any_gpt(body)
}

fn discover_mbr_partitions(body: &mut Body) -> Result<mbr::MBR, Box<dyn Error>> {
    let sector_size = body.get_sector_size() as usize;
    if sector_size < 512 {
        return Err(
            format!("sector size {sector_size} is too small to contain an MBR sector").into(),
        );
    }
    let mut bootsector: Vec<u8> = vec![0; sector_size];
    body.seek(SeekFrom::Start(0))?;
    body.read_exact(&mut bootsector)?;
    let main_mbr = mbr::MBR::from_bytes(&bootsector);
    if main_mbr.is_mbr() {
        info!("Detected an MBR partition scheme.");
        if main_mbr.is_pmbr() {
            info!("Detected a Protective MBR. GPT scheme is strongly suspected.")
        }
        Ok(main_mbr)
    } else {
        warn!("No MBR signature found");
        Err("No MBR signature found".into())
    }
}

pub fn read_gpt_header_at(body: &mut Body, lba: u64) -> Result<GPTHeader, Box<dyn Error>> {
    let sector_size = body.get_sector_size() as u64;
    let image_size_bytes = body.get_image_size();
    validate_sector_and_image_size(sector_size, image_size_bytes)?;

    let header_offset = lba
        .checked_mul(sector_size)
        .ok_or(GptParseError::ArithmeticOverflow("GPT header offset"))?;
    checked_image_range(
        "GPT header sector",
        header_offset,
        sector_size,
        image_size_bytes,
    )?;

    let mut hdr_bytes = vec![0u8; sector_size as usize];
    body.seek(SeekFrom::Start(header_offset))?;
    body.read_exact(&mut hdr_bytes)?;
    let gpt = GPT::try_from_bytes(&hdr_bytes)?;
    validate_gpt_header(&gpt.header, &hdr_bytes, lba, sector_size, image_size_bytes)?;
    Ok(gpt.header)
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
    let image_size_bytes = body.get_image_size();
    validate_sector_and_image_size(sector_size, image_size_bytes)?;
    let total_sectors = image_size_bytes / sector_size;

    // Primary GPT header is always at LBA 1
    // Backup GPT header is always at the last LBA of the disk / image
    let target_lba = if backup {
        total_sectors - 1 // last complete LBA
    } else {
        1 // primary header
    };

    let header_offset = target_lba
        .checked_mul(sector_size)
        .ok_or(GptParseError::ArithmeticOverflow("GPT header offset"))?;
    checked_image_range(
        "GPT header sector",
        header_offset,
        sector_size,
        image_size_bytes,
    )?;

    let mut hdr_raw = vec![0u8; sector_size as usize];
    body.seek(SeekFrom::Start(header_offset))?;
    body.read_exact(&mut hdr_raw)?;

    let mut gpt = GPT::try_from_bytes(&hdr_raw)?;
    let table = validate_gpt_header(
        &gpt.header,
        &hdr_raw,
        target_lba,
        sector_size,
        image_size_bytes,
    )?;

    info!(
        "Discovered a {} GPT header at LBA {}",
        if backup { "backup" } else { "primary" },
        target_lba
    );

    let num_entries = gpt.header.num_partition_entries as usize;
    let entry_size = gpt.header.partition_entry_size as usize;
    let table_length = usize::try_from(table.length_bytes)
        .map_err(|_| GptParseError::ArithmeticOverflow("GPT partition-array allocation"))?;
    let mut table_bytes = vec![0u8; table_length];
    body.seek(SeekFrom::Start(table.offset_bytes))?;
    body.read_exact(&mut table_bytes)?;

    let actual_array_crc = crc32_ieee(&table_bytes);
    if actual_array_crc != gpt.header.partition_array_crc32 {
        return Err(GptParseError::PartitionArrayCrcMismatch {
            expected: gpt.header.partition_array_crc32,
            actual: actual_array_crc,
        }
        .into());
    }

    gpt.partition_entries = Vec::with_capacity(num_entries);
    let mut used_partition_guids = HashSet::new();

    for (i, entry_buf) in table_bytes.chunks_exact(entry_size).enumerate() {
        let index = u32::try_from(i)
            .map_err(|_| GptParseError::ArithmeticOverflow("GPT partition entry index"))?;
        let mut entry = GPTPartitionEntry::try_from_bytes(entry_buf).map_err(|error| {
            GptParseError::InvalidPartitionEntry {
                index,
                message: error.to_string(),
            }
        })?;
        entry.id = Some(i as i64);

        // Skip unused (all-zero) entries to keep the output tidy
        if entry.partition_type_guid != [0u8; 16] {
            validate_gpt_partition_entry(
                &mut entry,
                index,
                &gpt.header,
                sector_size,
                image_size_bytes,
            )?;
            if !used_partition_guids.insert(entry.partition_guid) {
                return Err(GptParseError::InvalidPartitionEntry {
                    index,
                    message: "duplicate partition GUID".to_owned(),
                }
                .into());
            }
            gpt.partition_entries.push(entry);
        }
    }

    Ok(gpt)
}

fn validate_sector_and_image_size(
    sector_size: u64,
    image_size_bytes: u64,
) -> Result<(), GptParseError> {
    if sector_size < GPT_HEADER_MIN_SIZE as u64 {
        return Err(GptParseError::InvalidHeader(format!(
            "sector size {sector_size} is smaller than the fixed GPT header"
        )));
    }
    if image_size_bytes < sector_size.saturating_mul(2) {
        return Err(GptParseError::InvalidHeader(format!(
            "image size {image_size_bytes} cannot contain an MBR and GPT header"
        )));
    }
    Ok(())
}

fn validate_gpt_header(
    header: &GPTHeader,
    header_sector: &[u8],
    expected_lba: u64,
    sector_size: u64,
    image_size_bytes: u64,
) -> Result<GptTableGeometry, GptParseError> {
    if header.signature != *b"EFI PART" {
        return Err(GptParseError::InvalidSignature);
    }
    if header.revision != 0x0001_0000 {
        return Err(GptParseError::InvalidHeader(format!(
            "unsupported revision {:#010x}",
            header.revision
        )));
    }

    let header_size = usize::try_from(header.header_size)
        .map_err(|_| GptParseError::ArithmeticOverflow("GPT header size"))?;
    let sector_size_usize = usize::try_from(sector_size)
        .map_err(|_| GptParseError::ArithmeticOverflow("GPT sector size"))?;
    if !(GPT_HEADER_MIN_SIZE..=sector_size_usize).contains(&header_size) {
        return Err(GptParseError::InvalidHeader(format!(
            "header size {} is outside the supported range {}..={sector_size}",
            header.header_size, GPT_HEADER_MIN_SIZE
        )));
    }
    if header_sector.len() < header_size {
        return Err(GptParseError::Truncated {
            structure: "GPT header",
            needed: header_size,
            actual: header_sector.len(),
        });
    }
    if header.reserved != 0 {
        return Err(GptParseError::InvalidHeader(
            "reserved field is not zero".to_owned(),
        ));
    }
    if header.current_lba != expected_lba {
        return Err(GptParseError::InvalidHeader(format!(
            "current LBA {} does not match header location {expected_lba}",
            header.current_lba
        )));
    }

    let total_sectors = image_size_bytes / sector_size;
    if header.disk_guid == [0u8; 16] {
        return Err(GptParseError::InvalidHeader(
            "disk GUID is all zero".to_owned(),
        ));
    }
    if header.backup_lba >= total_sectors || header.backup_lba == header.current_lba {
        return Err(GptParseError::InvalidHeader(format!(
            "backup LBA {} is invalid for an image with {total_sectors} complete sectors",
            header.backup_lba
        )));
    }
    if header.first_usable_lba > header.last_usable_lba || header.last_usable_lba >= total_sectors {
        return Err(GptParseError::InvalidHeader(format!(
            "usable LBA range {}..={} is invalid for an image with {total_sectors} complete sectors",
            header.first_usable_lba, header.last_usable_lba
        )));
    }

    let entry_count = header.num_partition_entries;
    if entry_count == 0 || entry_count > MAX_GPT_PARTITION_ENTRIES {
        return Err(GptParseError::InvalidHeader(format!(
            "partition-entry count {entry_count} is outside the supported range 1..={MAX_GPT_PARTITION_ENTRIES}"
        )));
    }

    let entry_size = header.partition_entry_size;
    let entry_size_multiple = entry_size / GPT_PARTITION_ENTRY_MIN_SIZE as u32;
    if entry_size < GPT_PARTITION_ENTRY_MIN_SIZE as u32
        || entry_size > MAX_GPT_PARTITION_ENTRY_SIZE
        || !entry_size.is_multiple_of(GPT_PARTITION_ENTRY_MIN_SIZE as u32)
        || !entry_size_multiple.is_power_of_two()
    {
        return Err(GptParseError::InvalidHeader(format!(
            "partition-entry size {entry_size} is invalid; expected 128 times a power of two, at most {MAX_GPT_PARTITION_ENTRY_SIZE}"
        )));
    }

    let table_length = u64::from(entry_count)
        .checked_mul(u64::from(entry_size))
        .ok_or(GptParseError::ArithmeticOverflow(
            "GPT partition-array length",
        ))?;
    if table_length > MAX_GPT_PARTITION_ARRAY_BYTES {
        return Err(GptParseError::InvalidHeader(format!(
            "partition array length {table_length} exceeds safety limit {MAX_GPT_PARTITION_ARRAY_BYTES}"
        )));
    }
    let table_offset = header.partition_entry_lba.checked_mul(sector_size).ok_or(
        GptParseError::ArithmeticOverflow("GPT partition-array offset"),
    )?;
    let table_end = checked_image_range(
        "GPT partition array",
        table_offset,
        table_length,
        image_size_bytes,
    )?;
    if header.partition_entry_lba == 0 {
        return Err(GptParseError::InvalidHeader(
            "partition array overlaps the protective MBR".to_owned(),
        ));
    }

    let rounded_table_length =
        table_length
            .checked_add(sector_size - 1)
            .ok_or(GptParseError::ArithmeticOverflow(
                "rounded GPT partition-array length",
            ))?;
    let table_sectors = rounded_table_length / sector_size;
    let table_end_lba = header
        .partition_entry_lba
        .checked_add(table_sectors)
        .ok_or(GptParseError::ArithmeticOverflow(
            "GPT partition-array ending LBA",
        ))?;
    let usable_end_lba =
        header
            .last_usable_lba
            .checked_add(1)
            .ok_or(GptParseError::ArithmeticOverflow(
                "GPT usable-range ending LBA",
            ))?;
    if header.partition_entry_lba < usable_end_lba && header.first_usable_lba < table_end_lba {
        return Err(GptParseError::InvalidHeader(
            "partition array overlaps the usable LBA range".to_owned(),
        ));
    }
    if (header.first_usable_lba..usable_end_lba).contains(&header.current_lba)
        || (header.first_usable_lba..usable_end_lba).contains(&header.backup_lba)
    {
        return Err(GptParseError::InvalidHeader(
            "usable LBA range overlaps a GPT header".to_owned(),
        ));
    }

    let header_offset = expected_lba
        .checked_mul(sector_size)
        .ok_or(GptParseError::ArithmeticOverflow("GPT header offset"))?;
    let header_end = header_offset
        .checked_add(sector_size)
        .ok_or(GptParseError::ArithmeticOverflow("GPT header sector end"))?;
    if table_offset < header_end && header_offset < table_end {
        return Err(GptParseError::InvalidHeader(
            "partition array overlaps the GPT header sector".to_owned(),
        ));
    }

    let backup_header_offset =
        header
            .backup_lba
            .checked_mul(sector_size)
            .ok_or(GptParseError::ArithmeticOverflow(
                "backup GPT header offset",
            ))?;
    let backup_header_end =
        backup_header_offset
            .checked_add(sector_size)
            .ok_or(GptParseError::ArithmeticOverflow(
                "backup GPT header sector end",
            ))?;
    if table_offset < backup_header_end && backup_header_offset < table_end {
        return Err(GptParseError::InvalidHeader(
            "partition array overlaps the alternate GPT header sector".to_owned(),
        ));
    }

    let mut header_for_crc = header_sector[..header_size].to_vec();
    header_for_crc[16..20].fill(0);
    let actual_header_crc = crc32_ieee(&header_for_crc);
    if actual_header_crc != header.crc32 {
        return Err(GptParseError::HeaderCrcMismatch {
            expected: header.crc32,
            actual: actual_header_crc,
        });
    }

    Ok(GptTableGeometry {
        offset_bytes: table_offset,
        length_bytes: table_length,
    })
}

fn validate_gpt_partition_entry(
    entry: &mut GPTPartitionEntry,
    index: u32,
    header: &GPTHeader,
    sector_size: u64,
    image_size_bytes: u64,
) -> Result<(), GptParseError> {
    if entry.partition_guid == [0u8; 16] {
        return Err(GptParseError::InvalidPartitionEntry {
            index,
            message: "used entry has an all-zero partition GUID".to_owned(),
        });
    }
    if entry.starting_lba > entry.ending_lba {
        return Err(GptParseError::InvalidPartitionEntry {
            index,
            message: format!(
                "starting LBA {} is after ending LBA {}",
                entry.starting_lba, entry.ending_lba
            ),
        });
    }
    if entry.starting_lba < header.first_usable_lba || entry.ending_lba > header.last_usable_lba {
        return Err(GptParseError::InvalidPartitionEntry {
            index,
            message: format!(
                "LBA range {}..={} is outside the header's usable range {}..={} ",
                entry.starting_lba,
                entry.ending_lba,
                header.first_usable_lba,
                header.last_usable_lba
            ),
        });
    }

    let length_sectors = entry
        .ending_lba
        .checked_sub(entry.starting_lba)
        .and_then(|difference| difference.checked_add(1))
        .ok_or_else(|| GptParseError::InvalidPartitionEntry {
            index,
            message: "inclusive LBA length overflows".to_owned(),
        })?;
    let start_bytes = entry.starting_lba.checked_mul(sector_size).ok_or_else(|| {
        GptParseError::InvalidPartitionEntry {
            index,
            message: "byte offset overflows".to_owned(),
        }
    })?;
    let length_bytes = length_sectors.checked_mul(sector_size).ok_or_else(|| {
        GptParseError::InvalidPartitionEntry {
            index,
            message: "byte length overflows".to_owned(),
        }
    })?;
    checked_image_range("GPT partition", start_bytes, length_bytes, image_size_bytes).map_err(
        |error| GptParseError::InvalidPartitionEntry {
            index,
            message: error.to_string(),
        },
    )?;

    entry.first_byte_addr = start_bytes;
    entry.size_sectors = length_sectors;
    Ok(())
}

fn checked_image_range(
    structure: &'static str,
    start_bytes: u64,
    length_bytes: u64,
    image_size_bytes: u64,
) -> Result<u64, GptParseError> {
    let end_bytes = start_bytes
        .checked_add(length_bytes)
        .ok_or(GptParseError::ArithmeticOverflow("GPT byte-range end"))?;
    if end_bytes > image_size_bytes {
        return Err(GptParseError::OutsideImage {
            structure,
            start_bytes,
            length_bytes,
            image_size_bytes,
        });
    }
    Ok(end_bytes)
}

fn crc32_ieee(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod gpt_discovery_tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    const SECTOR_SIZE: usize = 512;
    const IMAGE_SECTORS: usize = 100;
    const HEADER_OFFSET: usize = SECTOR_SIZE;
    const TABLE_OFFSET: usize = 2 * SECTOR_SIZE;
    const ENTRY_COUNT: usize = 4;
    const ENTRY_SIZE: usize = 128;

    fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
        bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }

    fn refresh_header_crc(image: &mut [u8]) {
        let header = &mut image[HEADER_OFFSET..HEADER_OFFSET + GPT_HEADER_MIN_SIZE];
        header[16..20].fill(0);
        let checksum = crc32_ieee(header);
        put_u32(image, HEADER_OFFSET + 16, checksum);
    }

    fn refresh_array_and_header_crc(image: &mut [u8]) {
        let checksum =
            crc32_ieee(&image[TABLE_OFFSET..TABLE_OFFSET + ENTRY_COUNT.saturating_mul(ENTRY_SIZE)]);
        put_u32(image, HEADER_OFFSET + 88, checksum);
        refresh_header_crc(image);
    }

    fn valid_primary_gpt_image() -> Vec<u8> {
        let mut image = vec![0u8; IMAGE_SECTORS * SECTOR_SIZE];

        // Primary header at LBA 1.
        image[HEADER_OFFSET..HEADER_OFFSET + 8].copy_from_slice(b"EFI PART");
        put_u32(&mut image, HEADER_OFFSET + 8, 0x0001_0000);
        put_u32(&mut image, HEADER_OFFSET + 12, GPT_HEADER_MIN_SIZE as u32);
        put_u64(&mut image, HEADER_OFFSET + 24, 1);
        put_u64(&mut image, HEADER_OFFSET + 32, (IMAGE_SECTORS - 1) as u64);
        put_u64(&mut image, HEADER_OFFSET + 40, 34);
        put_u64(&mut image, HEADER_OFFSET + 48, 97);
        image[HEADER_OFFSET + 56..HEADER_OFFSET + 72].copy_from_slice(&[0x42; 16]);
        put_u64(&mut image, HEADER_OFFSET + 72, 2);
        put_u32(&mut image, HEADER_OFFSET + 80, ENTRY_COUNT as u32);
        put_u32(&mut image, HEADER_OFFSET + 84, ENTRY_SIZE as u32);

        // One used entry followed by three unused entries.
        image[TABLE_OFFSET..TABLE_OFFSET + 16].copy_from_slice(&[0x11; 16]);
        image[TABLE_OFFSET + 16..TABLE_OFFSET + 32].copy_from_slice(&[0x22; 16]);
        put_u64(&mut image, TABLE_OFFSET + 32, 40);
        put_u64(&mut image, TABLE_OFFSET + 40, 49);
        let name: Vec<u8> = "Data".encode_utf16().flat_map(u16::to_le_bytes).collect();
        image[TABLE_OFFSET + 56..TABLE_OFFSET + 56 + name.len()].copy_from_slice(&name);

        refresh_array_and_header_crc(&mut image);
        image
    }

    fn with_raw_body<T>(image: &[u8], action: impl FnOnce(&mut Body) -> T) -> T {
        let mut file = NamedTempFile::new().expect("create raw GPT fixture");
        file.write_all(image).expect("write raw GPT fixture");
        file.flush().expect("flush raw GPT fixture");
        let mut body = Body::try_new(file.path().to_string_lossy().into_owned(), "raw")
            .expect("open raw GPT fixture");
        action(&mut body)
    }

    fn assert_primary_rejected(image: &[u8], expected_message: &str) {
        with_raw_body(image, |body| {
            let error = discover_gpt_partitions(body, false).unwrap_err();
            assert!(
                error.to_string().contains(expected_message),
                "unexpected error: {error}"
            );
        });
    }

    #[test]
    fn discovers_crc_valid_gpt_through_body() {
        with_raw_body(&valid_primary_gpt_image(), |body| {
            let partitions = Partitions::new(body).unwrap();
            let gpt = partitions.gpt.expect("valid GPT should be discovered");
            assert_eq!(gpt.partition_entries.len(), 1);
            assert_eq!(gpt.partition_entries[0].id, Some(0));
            assert_eq!(gpt.partition_entries[0].first_byte_addr, 40 * 512);
            assert_eq!(gpt.partition_entries[0].size_sectors, 10);
        });
    }

    #[test]
    fn gpt_only_discovery_ignores_malformed_extended_partition_chains() {
        let mut image = valid_primary_gpt_image();
        const FIRST_MBR_ENTRY: usize = 446;
        image[FIRST_MBR_ENTRY + 4] = 0x0f;
        put_u32(&mut image, FIRST_MBR_ENTRY + 8, u32::MAX);
        put_u32(&mut image, FIRST_MBR_ENTRY + 12, u32::MAX);
        image[510] = 0x55;
        image[511] = 0xaa;

        with_raw_body(&image, |body| {
            let gpt = discover_gpt(body).expect("GPT-only path must not parse EBR chains");
            assert_eq!(gpt.partition_entries.len(), 1);
            assert_eq!(gpt.partition_entries[0].id, Some(0));
        });
    }

    #[test]
    fn rejects_unbounded_entry_count_before_allocation() {
        let mut image = valid_primary_gpt_image();
        put_u32(&mut image, HEADER_OFFSET + 80, u32::MAX);
        refresh_header_crc(&mut image);
        assert_primary_rejected(&image, "partition-entry count");

        // The public discovery path falls back safely and reports no GPT; it
        // must not panic or attempt the attacker-controlled allocation.
        with_raw_body(&image, |body| {
            assert!(Partitions::new(body).unwrap().gpt.is_none());
        });
    }

    #[test]
    fn rejects_invalid_entry_size_and_out_of_image_table() {
        let mut invalid_size = valid_primary_gpt_image();
        put_u32(&mut invalid_size, HEADER_OFFSET + 84, 64);
        refresh_header_crc(&mut invalid_size);
        assert_primary_rejected(&invalid_size, "partition-entry size");

        let mut outside_image = valid_primary_gpt_image();
        put_u64(&mut outside_image, HEADER_OFFSET + 72, IMAGE_SECTORS as u64);
        refresh_header_crc(&mut outside_image);
        assert_primary_rejected(&outside_image, "exceeds image size");

        let mut overflow = valid_primary_gpt_image();
        put_u64(&mut overflow, HEADER_OFFSET + 72, u64::MAX);
        refresh_header_crc(&mut overflow);
        assert_primary_rejected(&overflow, "overflow");
    }

    #[test]
    fn rejects_header_and_partition_array_crc_corruption() {
        let mut bad_header = valid_primary_gpt_image();
        bad_header[HEADER_OFFSET + 16] ^= 0x01;
        assert_primary_rejected(&bad_header, "header CRC mismatch");

        let mut bad_array = valid_primary_gpt_image();
        bad_array[TABLE_OFFSET] ^= 0x01;
        assert_primary_rejected(&bad_array, "partition-array CRC mismatch");
    }

    #[test]
    fn rejects_malformed_used_partition_geometry() {
        let mut image = valid_primary_gpt_image();
        put_u64(&mut image, TABLE_OFFSET + 32, 60);
        put_u64(&mut image, TABLE_OFFSET + 40, 50);
        refresh_array_and_header_crc(&mut image);
        assert_primary_rejected(&image, "starting LBA 60 is after ending LBA 50");
    }

    #[test]
    fn legacy_decoders_are_non_panicking_on_truncated_data() {
        assert!(!GPT::from_bytes(&[]).is_gpt());
        assert_eq!(
            GPTPartitionEntry::from_bytes(&[]).partition_type_guid,
            [0u8; 16]
        );
        assert!(matches!(
            GPT::try_from_bytes(&[]),
            Err(GptParseError::Truncated { .. })
        ));
        assert!(matches!(
            GPTPartitionEntry::try_from_bytes(&[]),
            Err(GptParseError::Truncated { .. })
        ));
    }

    #[test]
    fn truncated_image_is_rejected_without_panicking() {
        with_raw_body(&[0u8; 100], |body| {
            let partitions = Partitions::new(body).unwrap();
            assert!(partitions.mbr.is_none());
            assert!(partitions.gpt.is_none());
        });
    }

    #[test]
    fn crc32_implementation_matches_standard_vector() {
        assert_eq!(crc32_ieee(b"123456789"), 0xcbf4_3926);
    }
}
