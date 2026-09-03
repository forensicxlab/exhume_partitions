//! Checked, normalized partition extents for block-volume consumers.
//!
//! The partition parsers expose their native on-disk structures.  Exporters and
//! filesystem readers instead need one unambiguous byte range that has already
//! been checked against the logical image size.  This module provides that
//! boundary without changing the existing `Partitions`, `GPT`, or MBR output.

use crate::gpt::{format_guid, GPTPartitionEntry, GPT};
use crate::Partitions;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;

/// The on-disk partition-table scheme from which an extent originated.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum PartitionScheme {
    Gpt,
    Mbr,
    Ebr,
}

/// A normalized, image-relative byte range for a partition or volume.
///
/// `id` is the original zero-based slot in the source partition table.  It is
/// deliberately not the position in a filtered `Vec` of used partitions.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PartitionExtent {
    pub scheme: PartitionScheme,
    pub id: u32,
    pub partition_guid: Option<String>,
    pub partition_name: Option<String>,
    pub partition_type: String,
    pub partition_type_guid: Option<String>,
    pub start_bytes: u64,
    pub length_bytes: u64,
    pub sector_size: u64,
}

/// Alias used by block-volume pipelines, where the partition-table origin is
/// useful provenance but the range is consumed as a volume.
pub type VolumeExtent = PartitionExtent;

impl PartitionExtent {
    /// Build and validate an extent from a used GPT entry.
    ///
    /// All LBA arithmetic is checked and the exclusive end of the partition
    /// must be within `image_size_bytes`.
    pub fn from_gpt_entry(
        entry: &GPTPartitionEntry,
        sector_size: u64,
        image_size_bytes: u64,
    ) -> Result<Self, PartitionExtentError> {
        if sector_size == 0 {
            return Err(PartitionExtentError::InvalidSectorSize);
        }

        let raw_id = entry.id.ok_or(PartitionExtentError::MissingPartitionId)?;
        let id =
            u32::try_from(raw_id).map_err(|_| PartitionExtentError::InvalidPartitionId(raw_id))?;

        let length_sectors = entry
            .ending_lba
            .checked_sub(entry.starting_lba)
            .and_then(|difference| difference.checked_add(1))
            .ok_or(PartitionExtentError::InvalidLbaRange {
                starting_lba: entry.starting_lba,
                ending_lba: entry.ending_lba,
            })?;

        let start_bytes = entry
            .starting_lba
            .checked_mul(sector_size)
            .ok_or(PartitionExtentError::ArithmeticOverflow("partition start"))?;
        let length_bytes = length_sectors
            .checked_mul(sector_size)
            .ok_or(PartitionExtentError::ArithmeticOverflow("partition length"))?;
        let end_bytes = start_bytes
            .checked_add(length_bytes)
            .ok_or(PartitionExtentError::ArithmeticOverflow("partition end"))?;

        if end_bytes > image_size_bytes {
            return Err(PartitionExtentError::OutsideImage {
                start_bytes,
                length_bytes,
                end_bytes,
                image_size_bytes,
            });
        }

        let partition_name = entry
            .partition_name
            .trim_end_matches('\0')
            .trim()
            .to_owned();

        Ok(Self {
            scheme: PartitionScheme::Gpt,
            id,
            partition_guid: Some(format_guid(&entry.partition_guid)),
            partition_name: (!partition_name.is_empty()).then_some(partition_name),
            partition_type: entry.partition_type_description().to_owned(),
            partition_type_guid: Some(format_guid(&entry.partition_type_guid)),
            start_bytes,
            length_bytes,
            sector_size,
        })
    }

    /// Exclusive end of this extent, checked even for deserialized instances.
    pub fn end_bytes(&self) -> Result<u64, PartitionExtentError> {
        self.start_bytes
            .checked_add(self.length_bytes)
            .ok_or(PartitionExtentError::ArithmeticOverflow("partition end"))
    }
}

/// Stable selectors for GPT partitions.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum GptPartitionSelector {
    /// Original zero-based GPT entry slot, not the filtered vector index.
    Index(u32),
    /// Canonical `8-4-4-4-12` GPT partition GUID (case-insensitive hex).
    Guid(String),
}

/// Errors returned while resolving or normalizing a partition extent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartitionExtentError {
    GptNotFound,
    InvalidCanonicalGuid(String),
    PartitionNotFound(String),
    AmbiguousPartition(String),
    MissingPartitionId,
    InvalidPartitionId(i64),
    InvalidSectorSize,
    InvalidLbaRange {
        starting_lba: u64,
        ending_lba: u64,
    },
    ArithmeticOverflow(&'static str),
    OutsideImage {
        start_bytes: u64,
        length_bytes: u64,
        end_bytes: u64,
        image_size_bytes: u64,
    },
}

impl fmt::Display for PartitionExtentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GptNotFound => write!(f, "no GPT partition table is available"),
            Self::InvalidCanonicalGuid(guid) => write!(
                f,
                "invalid canonical partition GUID {guid:?}; expected 8-4-4-4-12 hexadecimal form"
            ),
            Self::PartitionNotFound(selector) => {
                write!(f, "GPT partition {selector} was not found")
            }
            Self::AmbiguousPartition(selector) => {
                write!(f, "more than one GPT partition matches {selector}")
            }
            Self::MissingPartitionId => write!(f, "GPT partition has no original entry index"),
            Self::InvalidPartitionId(id) => {
                write!(f, "GPT partition entry index {id} cannot be represented as u32")
            }
            Self::InvalidSectorSize => write!(f, "sector size must be greater than zero"),
            Self::InvalidLbaRange {
                starting_lba,
                ending_lba,
            } => write!(
                f,
                "invalid GPT LBA range: start {starting_lba} is after end {ending_lba}, or its inclusive length overflows"
            ),
            Self::ArithmeticOverflow(field) => {
                write!(f, "integer overflow while calculating {field}")
            }
            Self::OutsideImage {
                start_bytes,
                length_bytes,
                end_bytes,
                image_size_bytes,
            } => write!(
                f,
                "partition byte range [{start_bytes}, {end_bytes}) (length {length_bytes}) exceeds image size {image_size_bytes}"
            ),
        }
    }
}

impl Error for PartitionExtentError {}

impl GPT {
    /// Resolve a checked GPT extent using a stable original slot or GUID.
    pub fn resolve_extent(
        &self,
        selector: &GptPartitionSelector,
        sector_size: u64,
        image_size_bytes: u64,
    ) -> Result<VolumeExtent, PartitionExtentError> {
        let (matches, display): (Vec<&GPTPartitionEntry>, String) = match selector {
            GptPartitionSelector::Index(index) => (
                self.partition_entries
                    .iter()
                    .filter(|entry| entry.id == Some(i64::from(*index)))
                    .collect(),
                format!("at original index {index}"),
            ),
            GptPartitionSelector::Guid(guid) => {
                let canonical = normalize_canonical_guid(guid)?;
                (
                    self.partition_entries
                        .iter()
                        .filter(|entry| format_guid(&entry.partition_guid) == canonical)
                        .collect(),
                    format!("with GUID {canonical}"),
                )
            }
        };

        match matches.as_slice() {
            [] => Err(PartitionExtentError::PartitionNotFound(display)),
            [entry] => PartitionExtent::from_gpt_entry(entry, sector_size, image_size_bytes),
            _ => Err(PartitionExtentError::AmbiguousPartition(display)),
        }
    }

    /// Resolve by the original zero-based GPT table slot.
    pub fn extent_by_index(
        &self,
        index: u32,
        sector_size: u64,
        image_size_bytes: u64,
    ) -> Result<VolumeExtent, PartitionExtentError> {
        self.resolve_extent(
            &GptPartitionSelector::Index(index),
            sector_size,
            image_size_bytes,
        )
    }

    /// Resolve by canonical GPT partition GUID.
    pub fn extent_by_guid(
        &self,
        guid: impl Into<String>,
        sector_size: u64,
        image_size_bytes: u64,
    ) -> Result<VolumeExtent, PartitionExtentError> {
        self.resolve_extent(
            &GptPartitionSelector::Guid(guid.into()),
            sector_size,
            image_size_bytes,
        )
    }
}

impl Partitions {
    /// Resolve a checked GPT extent from a discovered partition set.
    pub fn resolve_gpt_extent(
        &self,
        selector: &GptPartitionSelector,
        sector_size: u64,
        image_size_bytes: u64,
    ) -> Result<VolumeExtent, PartitionExtentError> {
        self.gpt
            .as_ref()
            .ok_or(PartitionExtentError::GptNotFound)?
            .resolve_extent(selector, sector_size, image_size_bytes)
    }

    /// Resolve by the original zero-based GPT table slot.
    pub fn gpt_extent_by_index(
        &self,
        index: u32,
        sector_size: u64,
        image_size_bytes: u64,
    ) -> Result<VolumeExtent, PartitionExtentError> {
        self.resolve_gpt_extent(
            &GptPartitionSelector::Index(index),
            sector_size,
            image_size_bytes,
        )
    }

    /// Resolve by canonical GPT partition GUID.
    pub fn gpt_extent_by_guid(
        &self,
        guid: impl Into<String>,
        sector_size: u64,
        image_size_bytes: u64,
    ) -> Result<VolumeExtent, PartitionExtentError> {
        self.resolve_gpt_extent(
            &GptPartitionSelector::Guid(guid.into()),
            sector_size,
            image_size_bytes,
        )
    }
}

fn normalize_canonical_guid(guid: &str) -> Result<String, PartitionExtentError> {
    let bytes = guid.as_bytes();
    let valid = bytes.len() == 36
        && [8, 13, 18, 23]
            .into_iter()
            .all(|position| bytes[position] == b'-')
        && bytes.iter().enumerate().all(|(position, byte)| {
            [8, 13, 18, 23].contains(&position) || byte.is_ascii_hexdigit()
        });

    if !valid {
        return Err(PartitionExtentError::InvalidCanonicalGuid(guid.to_owned()));
    }

    Ok(guid.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpt::{GPTHeader, GPTPartitionEntry};

    const BASIC_DATA_GUID: [u8; 16] = [
        0xa2, 0xa0, 0xd0, 0xeb, 0xe5, 0xb9, 0x33, 0x44, 0x87, 0xc0, 0x68, 0xb6, 0xb7, 0x26, 0x99,
        0xc7,
    ];
    const FIRST_GUID: [u8; 16] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10,
    ];
    const SECOND_GUID: [u8; 16] = [
        0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
        0x20,
    ];

    fn entry(id: i64, guid: [u8; 16], start: u64, end: u64) -> GPTPartitionEntry {
        GPTPartitionEntry {
            id: Some(id),
            partition_guid: guid,
            partition_type_guid: BASIC_DATA_GUID,
            starting_lba: start,
            ending_lba: end,
            partition_name: "Data\0\0".to_owned(),
            ..GPTPartitionEntry::default()
        }
    }

    fn sparse_gpt() -> GPT {
        // Unused entries 1 and 2 were filtered by discovery.  IDs still refer
        // to original GPT table slots, so the second Vec item has ID 3.
        GPT {
            header: GPTHeader::default(),
            partition_entries: vec![
                entry(0, FIRST_GUID, 40, 49),
                entry(3, SECOND_GUID, 100, 199),
            ],
        }
    }

    #[test]
    fn sparse_gpt_lookup_uses_original_entry_id() {
        let gpt = sparse_gpt();
        let extent = gpt.extent_by_index(3, 512, 512 * 1_000).unwrap();

        assert_eq!(extent.id, 3);
        assert_eq!(extent.start_bytes, 100 * 512);
        assert_eq!(extent.length_bytes, 100 * 512);
        assert_eq!(extent.partition_name.as_deref(), Some("Data"));
        assert_eq!(extent.partition_type, "Basic data partition");

        assert!(matches!(
            gpt.extent_by_index(1, 512, 512 * 1_000),
            Err(PartitionExtentError::PartitionNotFound(_))
        ));
    }

    #[test]
    fn resolves_canonical_guid_case_insensitively() {
        let gpt = sparse_gpt();
        let canonical = format_guid(&SECOND_GUID);
        let extent = gpt
            .extent_by_guid(canonical.to_ascii_uppercase(), 512, 512 * 1_000)
            .unwrap();

        assert_eq!(extent.id, 3);
        assert_eq!(extent.partition_guid.as_deref(), Some(canonical.as_str()));
    }

    #[test]
    fn rejects_noncanonical_guid() {
        let error = sparse_gpt()
            .extent_by_guid("{14131211-1615-1817-191a-1b1c1d1e1f20}", 512, 512_000)
            .unwrap_err();
        assert!(matches!(
            error,
            PartitionExtentError::InvalidCanonicalGuid(_)
        ));
    }

    #[test]
    fn rejects_zero_sector_size_and_invalid_lba_range() {
        assert_eq!(
            PartitionExtent::from_gpt_entry(&entry(0, FIRST_GUID, 1, 1), 0, 512),
            Err(PartitionExtentError::InvalidSectorSize)
        );
        assert!(matches!(
            PartitionExtent::from_gpt_entry(&entry(0, FIRST_GUID, 10, 9), 512, 512_000),
            Err(PartitionExtentError::InvalidLbaRange { .. })
        ));
    }

    #[test]
    fn rejects_start_length_and_end_overflow() {
        assert_eq!(
            PartitionExtent::from_gpt_entry(&entry(0, FIRST_GUID, u64::MAX, u64::MAX), 2, u64::MAX,),
            Err(PartitionExtentError::ArithmeticOverflow("partition start"))
        );
        assert!(matches!(
            PartitionExtent::from_gpt_entry(&entry(0, FIRST_GUID, 0, u64::MAX / 2), 3, u64::MAX,),
            Err(PartitionExtentError::ArithmeticOverflow("partition length"))
        ));
        assert_eq!(
            PartitionExtent::from_gpt_entry(&entry(0, FIRST_GUID, 1, u64::MAX), 1, u64::MAX,),
            Err(PartitionExtentError::ArithmeticOverflow("partition end"))
        );
    }

    #[test]
    fn rejects_partition_outside_logical_image() {
        let error =
            PartitionExtent::from_gpt_entry(&entry(0, FIRST_GUID, 100, 199), 512, 199 * 512)
                .unwrap_err();

        assert_eq!(
            error,
            PartitionExtentError::OutsideImage {
                start_bytes: 100 * 512,
                length_bytes: 100 * 512,
                end_bytes: 200 * 512,
                image_size_bytes: 199 * 512,
            }
        );
    }

    #[test]
    fn reports_missing_gpt_on_partitions_wrapper() {
        let partitions = Partitions {
            mbr: None,
            ebr: None,
            gpt: None,
        };

        assert_eq!(
            partitions.gpt_extent_by_index(0, 512, 512_000),
            Err(PartitionExtentError::GptNotFound)
        );
    }
}
