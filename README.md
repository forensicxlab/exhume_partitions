# Exhume Partitions

The Exhume Partitions documentation is maintained at [forensicxlab.com](https://www.forensicxlab.com/docs/exhume_partitions/getting-started).

## GPT forensic inspection

Opt in to a read-only inspection of the primary and backup GPTs:

```sh
exhume_partitions -b aircraft.bin --forensic
exhume_partitions -b aircraft.bin --forensic --json --output inspection.json
```

The `gpt_inspection.v1` report retains nonconforming used entries with their
original zero-based slots, records checksum and validation findings, and exposes
validated byte extents only for eligible entries. For example, an entry covering
sector zero is observed with `extent: null`; unrelated valid entries remain
available. No partition names receive vendor-specific exceptions.

Human-readable reports display LBA addresses in hexadecimal with a `0x` prefix.
Each entry also shows its partition GUID, type description and type GUID,
including excluded entries. JSON addresses remain numeric.

Both GPT copies are assessed independently. A damaged primary can fall back to
an integrity-valid backup. Conflicting integrity-valid copies expose no extents.
Invalid header checksums or geometry prevent following entry-array pointers;
bounded arrays with invalid checksums remain observations without extents.
Duplicate GUIDs, overlapping entries and metadata overlaps exclude every
implicated entry. Overlap findings identify representative conflicts, rather
than enumerating every pair. This mode inspects GPT only, not MBR/EBR chains,
and cannot be combined with `--bootloader`.

Exit codes in forensic mode are `0` for a clean report, `3` for a completed report
with findings, `1` for operational failures, and `2` for invalid arguments. A
missing GPT copy is a finding, including on MBR-only or unpartitioned images.
Unsupported sector geometry or an image smaller than two sectors is an
operational error. Actual read failures also prevent completion; the API does
not silently discard a source I/O error and label the report complete.
Findings are part of both text and JSON reports regardless of log level. JSON
stdout contains one report; logs use stderr. `--output` must name a new file in
forensic mode: existing files, symbolic links and hard links are refused
atomically. Existing discovery APIs, default CLI JSON and legacy output-file
overwrite behavior are unchanged.

### Library

```rust,no_run
use exhume_body::Body;
use exhume_partitions::inspect_gpt;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut body = Body::try_new("aircraft.bin".into(), "raw")?;
    let report = inspect_gpt(&mut body)?;
    for extent in report.validated_extents() {
        println!("slot {}: {} bytes at {}", extent.id, extent.length_bytes, extent.start_bytes);
    }
    println!("{} findings", report.issues.len());
    Ok(())
}
```

`inspect_gpt` accepts any `exhume_body::VolumeReader`, including `Body` and
bounded volume views. `report.selected_table()` provides normalized entry
observations; `report.validated_extents()` yields extents from the selected
copy only. `integrity_valid` describes header geometry and checksums, not the
validity of every partition. The report retains both copies and the decoded
headers for provenance. Inspection uses the reader's advertised logical sector
size (512 bytes for raw `Body` sources); it does not infer raw-image sector
geometry. It does not repair the evidence, scan arbitrary offsets
for lost tables, detect filesystems or establish the correctness of file content.
