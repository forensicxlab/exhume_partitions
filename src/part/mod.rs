use bootsector::{list_partitions, Attributes, Options, Partition, ReadGPT, ReadMBR, SectorSize};
use log::{debug, error, info};
use std::{collections::HashMap, str::FromStr};
use uguid::Guid;

pub struct MBR {
    flag: u8,
    ptype: u8,
    first_sector_addr: u32,
    size: u32,
}
pub struct VPartition {
    partition: Partition,
    os: String,
    description: String,
    compatible: bool,
}

impl MBR {
    pub fn new(data: &Vec<u8>) -> MBR {
        return MBR {
            flag: u8::from_le_bytes(data[0x00..0x01].try_into().unwrap()),
            ptype: u8::from_le_bytes(data[0x04..0x05].try_into().unwrap()),
            first_sector_addr: u32::from_le_bytes(data[0x08..0x0C].try_into().unwrap()),
            size: u32::from_le_bytes(data[0x0C..0x10].try_into().unwrap()),
        };
    }

    pub fn print_info(&self) {
        info!(
            "MBR : Boot=0x{:x}, Type=0x{:x}, start_addr=0x{:x}, size=0x{:x}",
            self.flag, self.ptype, self.first_sector_addr, self.size
        );
    }

    pub fn get_partition_size(&self) -> usize {
        return self.size as usize;
    }

    pub fn get_partition_type(&self) -> u8 {
        return self.ptype;
    }
}

impl VPartition {
    fn new(partition: Partition) -> Result<VPartition, String> {
        match partition.attributes {
            Attributes::GPT { type_uuid, .. } => {
                let guids = HashMap::from([
                    (
                        "Linux",
                        HashMap::from([
                            (
                                Guid::from_str("0FC63DAF-8483-4772-8E79-3D69D8477DE4").unwrap(),
                                "Linux filesystem data",
                            ),
                            (
                                Guid::from_str("A19D880F-05FC-4D3B-A006-743F0F84911E").unwrap(),
                                "RAID partition",
                            ),
                            (
                                Guid::from_str("44479540-F297-41B2-9AF7-D131D5F0458A").unwrap(),
                                "Root partition (x86)",
                            ),
                            (
                                Guid::from_str("4F68BCE3-E8CD-4DB1-96E7-FBCAF984B709").unwrap(),
                                "Root partition (x86-64)",
                            ),
                            (
                                Guid::from_str("69DAD710-2CE4-4E3C-B16C-21A1D49ABED3").unwrap(),
                                "Root partition (32-bit ARM)",
                            ),
                            (
                                Guid::from_str("B921B045-1DF0-41C3-AF44-4C6F280D3FAE").unwrap(),
                                "Root partition (64-bit ARM/AArch64)",
                            ),
                            (
                                Guid::from_str("BC13C2FF-59E6-4262-A352-B275FD6F7172").unwrap(),
                                "/boot partition",
                            ),
                            (
                                Guid::from_str("0657FD6D-A4AB-43C4-84E5-0933C84B4F4F").unwrap(),
                                "Swap partition",
                            ),
                            (
                                Guid::from_str("E6D6D379-F507-44C2-A23C-238F2A3DF928").unwrap(),
                                "Logical Volume Manager (LVM) partition",
                            ),
                            (
                                Guid::from_str("933AC7E1-2EB4-4F13-B844-0E14E2AEF915").unwrap(),
                                "/home partition",
                            ),
                            (
                                Guid::from_str("3B8F8425-20E0-4F3B-907F-1A25A76F98E8").unwrap(),
                                "/srv (server data) partition",
                            ),
                            (
                                Guid::from_str("7FFEC5C9-2D00-49B7-8941-3EA10A5586B7").unwrap(),
                                "Plain dm-crypt partition",
                            ),
                            (
                                Guid::from_str("CA7D7CCB-63ED-4C53-861C-1742536059CC").unwrap(),
                                "LUKS partition",
                            ),
                            (
                                Guid::from_str("8DA63339-0007-60C0-C436-083AC8230908").unwrap(),
                                "Reserved",
                            ),
                        ]),
                    ),
                    (
                        "Other",
                        HashMap::from([
                            (
                                Guid::from_str("00000000-0000-0000-0000-000000000000").unwrap(),
                                "Unused entry",
                            ),
                            (
                                Guid::from_str("C12A7328-F81F-11D2-BA4B-00A0C93EC93B").unwrap(),
                                "EFI System partition",
                            ),
                        ]),
                    ),
                ]);

                let guid = Guid::from_bytes(type_uuid);
                for os in guids.keys() {
                    if guids[os].contains_key(&guid) {
                        return Ok(VPartition {
                            partition: partition,
                            os: os.to_string(),
                            description: guids[os][&guid].to_string(),
                            compatible: true,
                        });
                    }
                }

                return Ok(VPartition {
                    partition: partition,
                    os: "Unknown".to_string(),
                    description: "Unknown partition type".to_string(),
                    compatible: false,
                });
            }

            Attributes::MBR { .. } => {
                return Ok(VPartition {
                    partition: partition,
                    os: "TODO".to_string(),
                    description: "TODO".to_string(),
                    compatible: false,
                })
            }
        }
    }

    pub fn get_first_byte(&self) -> usize {
        return self.partition.first_byte as usize;
    }

    pub fn print_info(&self) {
        match &self.partition.attributes {
            Attributes::GPT {
                type_uuid,
                partition_uuid,
                attributes,
                name,
            } => {
                debug!("GPT");
                debug!("Name: {:?}", name);
                debug!(
                    "Type GUID : {:?}, {:?}/{:?}",
                    Guid::from_bytes(*type_uuid).to_string(),
                    self.os,
                    self.description
                );
                debug!(
                    "Partition GUID : {:?}",
                    Guid::from_bytes(*partition_uuid).to_string()
                );
                debug!("Attributes: {:?}", attributes);
                debug!("First Byte: 0x{:x}", self.partition.first_byte);
                debug!("Len: {:?}", self.partition.len);
                debug!("--------------------------------------");
            }

            Attributes::MBR {
                type_code,
                bootable,
            } => {
                debug!("MBR");
                debug!("Type Code : {:x}", type_code);
                debug!("Bootable : {:?}", bootable);
                debug!("--------------------------------------");
            }
        }
        info!("Exhume compatibility : {}", self.compatible);
    }
}

pub fn parse_partitions(sector_size: u16, mut reader: Vec<u8>) -> Result<Vec<VPartition>, String> {
    let mut result: Vec<VPartition> = Vec::new();
    let partitions = match list_partitions(
        &mut reader,
        &Options {
            mbr: ReadMBR::Modern,
            gpt: ReadGPT::RevisionOne,
            sector_size: SectorSize::Known(sector_size),
        },
    ) {
        Ok(p) => p,
        Err(message) => return Err(message.to_string()),
    };

    for partition in partitions {
        match VPartition::new(partition) {
            Ok(p) => result.push(p),
            Err(msg) => {
                error!("Could not parse the partition : {}", msg);
                continue;
            }
        };
    }
    return Ok(result);
}
