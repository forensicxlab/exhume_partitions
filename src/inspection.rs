//! Read-only GPT observations, separate from strict partition discovery.
//!
//! An observed entry is not necessarily an acceptable volume. Only `extent`
//! values from the selected, integrity-valid copy are eligible for downstream
//! reads. No vendor names are special-cased and no on-disk structures are repaired.

use crate::gpt::{format_guid, GPTHeader, GPTPartitionEntry, GPT, GPT_HEADER_MIN_SIZE};
use crate::{crc32_ieee, validate_gpt_header_geometry, GptTableGeometry, PartitionExtent};
use exhume_body::VolumeReader;
use prettytable::{Cell, Row, Table};
use serde::Serialize;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::io::{self, SeekFrom};

pub const GPT_INSPECTION_SCHEMA: &str = "gpt_inspection.v1";
const MAX_SECTOR_SIZE: u64 = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GptCopy {
    Primary,
    Backup,
}

impl GptCopy {
    fn label(self) -> &'static str {
        match self {
            Self::Primary => "Primary",
            Self::Backup => "Backup",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectionStatus {
    Complete,
    CompletedWithIssues,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChecksumStatus {
    NotChecked,
    Valid,
    Invalid,
}

/// `NotChecked` is distinct from a failed checksum (e.g. no GPT signature).
#[derive(Debug, Clone, Serialize)]
pub struct ChecksumCheck {
    pub status: ChecksumStatus,
    pub stored: Option<u32>,
    pub computed: Option<u32>,
}

impl ChecksumCheck {
    fn not_checked() -> Self {
        Self {
            status: ChecksumStatus::NotChecked,
            stored: None,
            computed: None,
        }
    }

    fn checked(stored: u32, computed: u32) -> Self {
        Self {
            status: if stored == computed {
                ChecksumStatus::Valid
            } else {
                ChecksumStatus::Invalid
            },
            stored: Some(stored),
            computed: Some(computed),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GptIssueCode {
    MissingSignature,
    InvalidHeader,
    HeaderChecksumMismatch,
    EntryArrayChecksumMismatch,
    InvalidPartitionGuid,
    DuplicatePartitionGuid,
    InvalidLbaRange,
    OutsideUsableRange,
    OutsideImage,
    ArithmeticOverflow,
    MetadataOverlap,
    PartitionOverlap,
    CopiesDisagree,
    ImageNotSectorAligned,
}

impl GptIssueCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingSignature => "missing_signature",
            Self::InvalidHeader => "invalid_header",
            Self::HeaderChecksumMismatch => "header_checksum_mismatch",
            Self::EntryArrayChecksumMismatch => "entry_array_checksum_mismatch",
            Self::InvalidPartitionGuid => "invalid_partition_guid",
            Self::DuplicatePartitionGuid => "duplicate_partition_guid",
            Self::InvalidLbaRange => "invalid_lba_range",
            Self::OutsideUsableRange => "outside_usable_range",
            Self::OutsideImage => "outside_image",
            Self::ArithmeticOverflow => "arithmetic_overflow",
            Self::MetadataOverlap => "metadata_overlap",
            Self::PartitionOverlap => "partition_overlap",
            Self::CopiesDisagree => "copies_disagree",
            Self::ImageNotSectorAligned => "image_not_sector_aligned",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GptInspectionIssue {
    pub code: GptIssueCode,
    pub source: Option<GptCopy>,
    pub slot: Option<u32>,
    /// A representative conflicting entry; overlap findings do not enumerate
    /// every pair, keeping work and output bounded even for nested ranges.
    pub related_slot: Option<u32>,
    pub message: String,
}

/// Normalized observation of a used on-disk entry. Slots are never renumbered.
#[derive(Debug, Clone, Serialize)]
pub struct GptEntryInspection {
    pub slot: u32,
    pub name: String,
    pub partition_type_guid: String,
    pub partition_guid: String,
    pub starting_lba: u64,
    pub ending_lba: u64,
    pub attributes: u64,
    /// None for invalid entries, checksum failures or conflicting GPT copies.
    pub extent: Option<PartitionExtent>,
    pub issues: Vec<GptIssueCode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GptTableInspection {
    pub source: GptCopy,
    pub header_lba: u64,
    pub header: Option<GPTHeader>,
    pub header_checksum: ChecksumCheck,
    pub entry_array_checksum: ChecksumCheck,
    /// Header geometry and both checksums pass. Individual entries may still
    /// be invalid; this field alone does not establish extent eligibility.
    pub integrity_valid: bool,
    pub entries: Vec<GptEntryInspection>,
}

/// GPT-only report. MBR/EBR discovery remains available via `Partitions::new`.
/// Reports are observations; the API neither mutates the source nor repairs GPTs.
#[derive(Debug, Clone, Serialize)]
pub struct GptInspection {
    pub schema: &'static str,
    pub mode: &'static str,
    pub status: InspectionStatus,
    pub image_size_bytes: u64,
    pub sector_size: u64,
    pub selected_source: Option<GptCopy>,
    pub copies: Vec<GptTableInspection>,
    pub issues: Vec<GptInspectionIssue>,
}

impl GptInspection {
    pub fn has_issues(&self) -> bool {
        !self.issues.is_empty()
    }

    pub fn selected_table(&self) -> Option<&GptTableInspection> {
        self.copies
            .iter()
            .find(|table| Some(table.source) == self.selected_source)
    }

    /// Eligible extents from the selected copy only; never duplicates from
    /// the redundant GPT or observations from conflicting tables.
    pub fn validated_extents(&self) -> impl Iterator<Item = &PartitionExtent> {
        self.selected_table().into_iter().flat_map(|table| {
            table
                .entries
                .iter()
                .filter_map(|entry| entry.extent.as_ref())
        })
    }

    pub fn print_info(&self) -> String {
        let mut output = String::from("GPT forensic inspection (gpt_inspection.v1)\n");
        output.push_str(&format!(
            "Selected source: {}\nImage bytes: {} | Sector bytes: {}\n",
            self.selected_source.map(GptCopy::label).unwrap_or("None"),
            self.image_size_bytes,
            self.sector_size
        ));
        for copy in &self.copies {
            output.push_str(&format!(
                "\n{} GPT at LBA {:#X}\nHeader checksum: {:?}\nEntry-array checksum: {:?}\nIntegrity valid: {}\nUsed entries: {} | Eligible extents: {}\n",
                copy.source.label(), copy.header_lba, copy.header_checksum.status,
                copy.entry_array_checksum.status, copy.integrity_valid, copy.entries.len(),
                copy.entries.iter().filter(|entry| entry.extent.is_some()).count()
            ));
            let mut table = Table::new();
            table.add_row(Row::new(
                [
                    "Slot",
                    "Name",
                    "Partition GUID",
                    "Type",
                    "Type GUID",
                    "Start LBA",
                    "End LBA",
                    "Extent eligibility",
                ]
                .into_iter()
                .map(Cell::new)
                .collect(),
            ));
            for entry in &copy.entries {
                let eligibility = if entry.extent.is_some() {
                    "Eligible".to_owned()
                } else {
                    format!(
                        "Excluded: {}",
                        entry
                            .issues
                            .iter()
                            .map(|code| code.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                table.add_row(Row::new(vec![
                    Cell::new(&entry.slot.to_string()),
                    Cell::new(&entry.name),
                    Cell::new(&entry.partition_guid),
                    Cell::new(GPTPartitionEntry::type_description_from_guid(
                        &entry.partition_type_guid,
                    )),
                    Cell::new(&entry.partition_type_guid),
                    Cell::new(&format!("{:#X}", entry.starting_lba)),
                    Cell::new(&format!("{:#X}", entry.ending_lba)),
                    Cell::new(&eligibility),
                ]));
            }
            output.push_str(&table.to_string());
        }
        output.push_str("\nFindings:\n");
        if self.issues.is_empty() {
            output.push_str("  None\n");
        }
        for issue in &self.issues {
            output.push_str(&format!(
                "  [{}] {}{}: {}\n",
                issue.code.as_str(),
                issue.source.map(GptCopy::label).unwrap_or("Image"),
                issue
                    .slot
                    .map(|slot| format!(" entry {slot}"))
                    .unwrap_or_default(),
                issue.message
            ));
        }
        output
    }
}

/// Failures that prevent an inspection from completing. Malformed on-disk
/// structures are returned as report findings, not operational errors.
#[derive(Debug)]
pub enum GptInspectionError {
    InvalidSectorSize(u64),
    ImageTooSmall {
        image_size_bytes: u64,
        sector_size: u64,
    },
    Io {
        offset_bytes: u64,
        source: io::Error,
    },
}

impl fmt::Display for GptInspectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSectorSize(size) => write!(
                f,
                "sector size {size} must be a power of two between 512 and {MAX_SECTOR_SIZE}"
            ),
            Self::ImageTooSmall {
                image_size_bytes,
                sector_size,
            } => write!(
                f,
                "image size {image_size_bytes} cannot hold two {sector_size}-byte sectors"
            ),
            Self::Io {
                offset_bytes,
                source,
            } => write!(
                f,
                "cannot read GPT evidence at byte {offset_bytes}: {source}"
            ),
        }
    }
}

impl Error for GptInspectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

struct AssessedCopy {
    report: GptTableInspection,
    geometry: Option<GptTableGeometry>,
    array: Vec<u8>,
}

/// Inspect both standard GPT locations through a bounded Exhume volume reader.
/// Header/array allocation limits are shared with strict discovery. This does
/// not scan for orphaned tables or invoke the legacy recursive EBR parser.
///
/// ```no_run
/// use exhume_body::Body;
/// use exhume_partitions::inspect_gpt;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut body = Body::try_new("aircraft.bin".into(), "raw")?;
/// let report = inspect_gpt(&mut body)?;
/// for extent in report.validated_extents() {
///     println!("slot {} at byte {}", extent.id, extent.start_bytes);
/// }
/// # Ok(())
/// # }
/// ```
pub fn inspect_gpt<R: VolumeReader + ?Sized>(
    reader: &mut R,
) -> Result<GptInspection, GptInspectionError> {
    let sector_size = u64::from(reader.sector_size());
    let image_size = reader.volume_len();
    if !(512..=MAX_SECTOR_SIZE).contains(&sector_size) || !sector_size.is_power_of_two() {
        return Err(GptInspectionError::InvalidSectorSize(sector_size));
    }
    if image_size / sector_size < 2 {
        return Err(GptInspectionError::ImageTooSmall {
            image_size_bytes: image_size,
            sector_size,
        });
    }
    let mut issues = Vec::new();
    if !image_size.is_multiple_of(sector_size) {
        issues.push(GptInspectionIssue {
            code: GptIssueCode::ImageNotSectorAligned, source: None, slot: None, related_slot: None,
            message: "image has a partial trailing sector; backup inspection uses the last complete sector".into(),
        });
    }
    let mut primary = inspect_copy(
        reader,
        GptCopy::Primary,
        image_size,
        sector_size,
        &mut issues,
    )?;
    let mut backup = inspect_copy(
        reader,
        GptCopy::Backup,
        image_size,
        sector_size,
        &mut issues,
    )?;

    // Even if one copy is damaged, protect both standard headers and every
    // array whose location came from a checksum-valid, geometry-valid header.
    let last_lba = image_size / sector_size - 1;
    let mut reserved = vec![(0, 1), (1, 2), (last_lba, last_lba + 1)];
    for copy in [&primary, &backup] {
        if let Some(geometry) = copy.geometry {
            let start = geometry.offset_bytes / sector_size;
            let length = geometry.length_bytes.div_ceil(sector_size);
            reserved.push((start, start + length));
        }
    }
    for copy in [&mut primary, &mut backup] {
        for entry in &mut copy.report.entries {
            if entry.starting_lba <= entry.ending_lba
                && reserved
                    .iter()
                    .any(|&(start, end)| entry.starting_lba < end && start <= entry.ending_lba)
            {
                entry_issue(
                    entry,
                    copy.report.source,
                    GptIssueCode::MetadataOverlap,
                    None,
                    "entry overlaps the MBR, a GPT header or an assessed GPT entry array".into(),
                    &mut issues,
                );
            }
        }
    }

    let conflict = primary.report.integrity_valid
        && backup.report.integrity_valid
        && !copies_agree(&primary, &backup);
    let selected_source = if conflict {
        issues.push(GptInspectionIssue {
            code: GptIssueCode::CopiesDisagree,
            source: None,
            slot: None,
            related_slot: None,
            message: "integrity-valid primary and backup GPTs disagree; all extents are excluded"
                .into(),
        });
        for copy in [&mut primary, &mut backup] {
            for entry in &mut copy.report.entries {
                entry.extent = None;
                entry.issues.push(GptIssueCode::CopiesDisagree);
            }
        }
        None
    } else if primary.report.integrity_valid {
        Some(GptCopy::Primary)
    } else if backup.report.integrity_valid {
        Some(GptCopy::Backup)
    } else {
        None
    };
    Ok(GptInspection {
        schema: GPT_INSPECTION_SCHEMA,
        mode: "forensic",
        status: if issues.is_empty() {
            InspectionStatus::Complete
        } else {
            InspectionStatus::CompletedWithIssues
        },
        image_size_bytes: image_size,
        sector_size,
        selected_source,
        copies: vec![primary.report, backup.report],
        issues,
    })
}

fn read_at<R: VolumeReader + ?Sized>(
    reader: &mut R,
    offset: u64,
    buffer: &mut [u8],
) -> Result<(), GptInspectionError> {
    reader
        .seek(SeekFrom::Start(offset))
        .and_then(|_| reader.read_exact(buffer))
        .map_err(|source| GptInspectionError::Io {
            offset_bytes: offset,
            source,
        })
}

fn table_issue(
    issues: &mut Vec<GptInspectionIssue>,
    source: GptCopy,
    code: GptIssueCode,
    message: String,
) {
    issues.push(GptInspectionIssue {
        code,
        source: Some(source),
        slot: None,
        related_slot: None,
        message,
    });
}

fn entry_issue(
    entry: &mut GptEntryInspection,
    source: GptCopy,
    code: GptIssueCode,
    related_slot: Option<u32>,
    message: String,
    issues: &mut Vec<GptInspectionIssue>,
) {
    entry.extent = None;
    if !entry.issues.contains(&code) {
        entry.issues.push(code);
        issues.push(GptInspectionIssue {
            code,
            source: Some(source),
            slot: Some(entry.slot),
            related_slot,
            message,
        });
    }
}

fn inspect_copy<R: VolumeReader + ?Sized>(
    reader: &mut R,
    source: GptCopy,
    image_size: u64,
    sector_size: u64,
    issues: &mut Vec<GptInspectionIssue>,
) -> Result<AssessedCopy, GptInspectionError> {
    let last_lba = image_size / sector_size - 1;
    let header_lba = if source == GptCopy::Primary {
        1
    } else {
        last_lba
    };
    let mut assessed = AssessedCopy {
        report: GptTableInspection {
            source,
            header_lba,
            header: None,
            header_checksum: ChecksumCheck::not_checked(),
            entry_array_checksum: ChecksumCheck::not_checked(),
            integrity_valid: false,
            entries: Vec::new(),
        },
        geometry: None,
        array: Vec::new(),
    };
    let mut bytes = vec![0u8; sector_size as usize];
    read_at(reader, header_lba * sector_size, &mut bytes)?;
    if bytes[..8] != *b"EFI PART" {
        table_issue(
            issues,
            source,
            GptIssueCode::MissingSignature,
            format!("no GPT signature at LBA {header_lba:#X}"),
        );
        return Ok(assessed);
    }
    // A whole sector is available; this decoder consumes only the fixed header.
    let header = match GPT::try_from_bytes(&bytes) {
        Ok(gpt) => gpt.header,
        Err(error) => {
            table_issue(
                issues,
                source,
                GptIssueCode::InvalidHeader,
                error.to_string(),
            );
            return Ok(assessed);
        }
    };
    assessed.report.header = Some(header.clone());
    if (GPT_HEADER_MIN_SIZE as u64..=sector_size).contains(&u64::from(header.header_size)) {
        let mut crc_bytes = bytes[..header.header_size as usize].to_vec();
        crc_bytes[16..20].fill(0);
        assessed.report.header_checksum =
            ChecksumCheck::checked(header.crc32, crc32_ieee(&crc_bytes));
        if assessed.report.header_checksum.status == ChecksumStatus::Invalid {
            table_issue(
                issues,
                source,
                GptIssueCode::HeaderChecksumMismatch,
                "stored and computed header CRC32 differ; entry-array pointers were not followed"
                    .into(),
            );
        }
    }
    let geometry =
        match validate_gpt_header_geometry(&header, &bytes, header_lba, sector_size, image_size) {
            Ok(geometry) => geometry,
            Err(error) => {
                table_issue(
                    issues,
                    source,
                    GptIssueCode::InvalidHeader,
                    error.to_string(),
                );
                return Ok(assessed);
            }
        };
    let counterpart = if source == GptCopy::Primary {
        last_lba
    } else {
        1
    };
    let array_start = geometry.offset_bytes / sector_size;
    let array_end = array_start + geometry.length_bytes.div_ceil(sector_size);
    if header.backup_lba != counterpart
        || header.first_usable_lba < 2
        || (source == GptCopy::Primary && (array_start < 2 || array_end > header.first_usable_lba))
        || (source == GptCopy::Backup
            && (array_start <= header.last_usable_lba || array_end > last_lba))
    {
        table_issue(issues, source, GptIssueCode::InvalidHeader,
            "header must identify its standard counterpart and place its entry array between its header and the usable range".into());
        return Ok(assessed);
    }
    if assessed.report.header_checksum.status != ChecksumStatus::Valid {
        return Ok(assessed);
    }
    assessed.geometry = Some(geometry);
    // Shared geometry checks cap this allocation at 64 MiB before this cast.
    assessed.array = vec![0u8; geometry.length_bytes as usize];
    read_at(reader, geometry.offset_bytes, &mut assessed.array)?;
    assessed.report.entry_array_checksum =
        ChecksumCheck::checked(header.partition_array_crc32, crc32_ieee(&assessed.array));
    assessed.report.integrity_valid =
        assessed.report.entry_array_checksum.status == ChecksumStatus::Valid;
    if !assessed.report.integrity_valid {
        table_issue(
            issues,
            source,
            GptIssueCode::EntryArrayChecksumMismatch,
            "stored and computed entry-array CRC32 differ; observations have no validated extents"
                .into(),
        );
    }
    for (slot, bytes) in assessed
        .array
        .chunks_exact(header.partition_entry_size as usize)
        .enumerate()
    {
        // Geometry enforces a complete array of entries of at least 128 bytes.
        let mut native =
            GPTPartitionEntry::try_from_bytes(bytes).map_err(|error| GptInspectionError::Io {
                offset_bytes: geometry.offset_bytes
                    + slot as u64 * u64::from(header.partition_entry_size),
                source: io::Error::new(io::ErrorKind::InvalidData, error),
            })?;
        if native.partition_type_guid == [0; 16] {
            continue;
        }
        native.id = Some(slot as i64);
        let mut entry = inspect_entry(&native, &header, source, sector_size, image_size, issues);
        if !assessed.report.integrity_valid {
            entry.extent = None;
            entry.issues.push(GptIssueCode::EntryArrayChecksumMismatch);
        }
        assessed.report.entries.push(entry);
    }
    validate_relationships(&mut assessed.report, issues);
    Ok(assessed)
}

fn inspect_entry(
    native: &GPTPartitionEntry,
    header: &GPTHeader,
    source: GptCopy,
    sector_size: u64,
    image_size: u64,
    issues: &mut Vec<GptInspectionIssue>,
) -> GptEntryInspection {
    let mut entry = GptEntryInspection {
        slot: native.id.unwrap_or_default() as u32,
        name: native.partition_name.trim_end_matches('\0').to_owned(),
        partition_type_guid: format_guid(&native.partition_type_guid),
        partition_guid: format_guid(&native.partition_guid),
        starting_lba: native.starting_lba,
        ending_lba: native.ending_lba,
        attributes: native.attributes,
        extent: None,
        issues: Vec::new(),
    };
    match PartitionExtent::from_gpt_entry(native, sector_size, image_size) {
        Ok(extent) => entry.extent = Some(extent),
        Err(error) => {
            use crate::PartitionExtentError;
            let code = match error {
                PartitionExtentError::OutsideImage { .. } => GptIssueCode::OutsideImage,
                PartitionExtentError::ArithmeticOverflow(_) => GptIssueCode::ArithmeticOverflow,
                _ => GptIssueCode::InvalidLbaRange,
            };
            entry_issue(&mut entry, source, code, None, error.to_string(), issues);
        }
    }
    if native.partition_guid == [0; 16] {
        entry_issue(
            &mut entry,
            source,
            GptIssueCode::InvalidPartitionGuid,
            None,
            "used entry has an all-zero partition GUID".into(),
            issues,
        );
    }
    if native.starting_lba < header.first_usable_lba || native.ending_lba > header.last_usable_lba {
        entry_issue(
            &mut entry,
            source,
            GptIssueCode::OutsideUsableRange,
            None,
            format!(
                "LBA range {:#X}..={:#X} is outside declared usable range {:#X}..={:#X}",
                native.starting_lba,
                native.ending_lba,
                header.first_usable_lba,
                header.last_usable_lba
            ),
            issues,
        );
    }
    entry
}

fn validate_relationships(table: &mut GptTableInspection, issues: &mut Vec<GptInspectionIssue>) {
    let mut guids: HashMap<String, usize> = HashMap::new();
    for index in 0..table.entries.len() {
        if table.entries[index]
            .issues
            .contains(&GptIssueCode::InvalidPartitionGuid)
        {
            continue;
        }
        if let Some(&other) = guids.get(&table.entries[index].partition_guid) {
            let slot = table.entries[index].slot;
            let other_slot = table.entries[other].slot;
            entry_issue(
                &mut table.entries[index],
                table.source,
                GptIssueCode::DuplicatePartitionGuid,
                Some(other_slot),
                format!("partition GUID also appears in entry {other_slot}"),
                issues,
            );
            entry_issue(
                &mut table.entries[other],
                table.source,
                GptIssueCode::DuplicatePartitionGuid,
                Some(slot),
                format!("partition GUID also appears in entry {slot}"),
                issues,
            );
        } else {
            guids.insert(table.entries[index].partition_guid.clone(), index);
        }
    }
    let mut ranges: Vec<usize> = (0..table.entries.len())
        .filter(|&index| table.entries[index].starting_lba <= table.entries[index].ending_lba)
        .collect();
    ranges.sort_unstable_by_key(|&index| {
        (
            table.entries[index].starting_lba,
            table.entries[index].ending_lba,
            table.entries[index].slot,
        )
    });
    let mut furthest: Option<usize> = None;
    for index in ranges {
        if let Some(other) = furthest {
            if table.entries[index].starting_lba <= table.entries[other].ending_lba {
                let slot = table.entries[index].slot;
                let other_slot = table.entries[other].slot;
                entry_issue(
                    &mut table.entries[index],
                    table.source,
                    GptIssueCode::PartitionOverlap,
                    Some(other_slot),
                    format!("LBA range overlaps entry {other_slot}"),
                    issues,
                );
                entry_issue(
                    &mut table.entries[other],
                    table.source,
                    GptIssueCode::PartitionOverlap,
                    Some(slot),
                    format!("LBA range overlaps entry {slot}"),
                    issues,
                );
            }
            if table.entries[index].ending_lba <= table.entries[other].ending_lba {
                continue;
            }
        }
        furthest = Some(index);
    }
}

fn copies_agree(primary: &AssessedCopy, backup: &AssessedCopy) -> bool {
    match (&primary.report.header, &backup.report.header) {
        (Some(a), Some(b)) => {
            a.backup_lba == b.current_lba
                && b.backup_lba == a.current_lba
                && a.disk_guid == b.disk_guid
                && a.first_usable_lba == b.first_usable_lba
                && a.last_usable_lba == b.last_usable_lba
                && a.num_partition_entries == b.num_partition_entries
                && a.partition_entry_size == b.partition_entry_size
                && primary.array == backup.array
        }
        _ => false,
    }
}
