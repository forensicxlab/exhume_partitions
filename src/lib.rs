pub mod gpt;
pub mod mbr;
use gpt::GPT;
use mbr::MBR;

pub fn is_gpt(bytes: &[u8]) -> bool {
    let gpt_signature = b"EFI PART";
    if bytes.len() < 512 {
        return false; // GPT header is expected to be at sector 1, starting at byte 512.
    }

    let gpt = GPT::from_bytes(&bytes);
    &gpt.header.signature == gpt_signature
}
