//! NTFS Master File Table records, parsed without touching the disk.
//!
//! The fast Windows scan (`mft.rs`) reads the whole MFT of a volume in large
//! sequential chunks and hands the raw records here. This module is pure, so
//! its tests run on every platform with hand-built records.
//!
//! Layouts follow the NTFS on-disk format: a `FILE` record header, then
//! attributes until the end marker. Records are protected by an update
//! sequence ("fixups"): the last two bytes of every 512-byte stride are
//! replaced on disk by a check value and stored in the header's array.

use std::ffi::OsString;
use std::io;

use rayon::prelude::*;

use super::listing::{self, Kind, Listing, Stat};

/// The root folder of every NTFS volume.
#[cfg(test)]
pub const ROOT_RECORD: u64 = 5;
/// Records below this number are NTFS metadata ($MFT, $Bitmap, $Extend…),
/// which directory listings never show.
const FIRST_USER_RECORD: u64 = 16;
const FIXUP_STRIDE: usize = 512;

const ATTR_STANDARD_INFORMATION: u32 = 0x10;
const ATTR_ATTRIBUTE_LIST: u32 = 0x20;
const ATTR_FILE_NAME: u32 = 0x30;
const ATTR_DATA: u32 = 0x80;
const ATTR_REPARSE_POINT: u32 = 0xC0;
const ATTR_END: u32 = 0xFFFF_FFFF;

const RECORD_IN_USE: u16 = 0x01;
const RECORD_IS_DIRECTORY: u16 = 0x02;

const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
/// `$FILE_NAME` namespaces.
const NAMESPACE_DOS: u8 = 2;
/// Attribute flags that mean the stored size differs from the allocation.
const ATTR_FLAG_COMPRESSED: u16 = 0x0001;
const ATTR_FLAG_SPARSE: u16 = 0x8000;

/// The low 48 bits of a file reference are the record number; the high 16
/// are its sequence number, which changes each time the record is reused.
const fn record_of(reference: u64) -> u64 {
    reference & 0x0000_FFFF_FFFF_FFFF
}
const fn sequence_of(reference: u64) -> u16 {
    (reference >> 48) as u16
}

fn u16_at(b: &[u8], off: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(off..off + 2)?.try_into().ok()?))
}
fn u32_at(b: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(off..off + 4)?.try_into().ok()?))
}
fn u64_at(b: &[u8], off: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(off..off + 8)?.try_into().ok()?))
}

/// Undo the update-sequence protection in place. Fails if the record is not
/// a `FILE` record or a sector was torn (written only in part).
pub fn apply_fixups(rec: &mut [u8]) -> Result<(), &'static str> {
    if rec.get(..4) != Some(b"FILE") {
        return Err("not a FILE record");
    }
    let usa_off = u16_at(rec, 4).ok_or("short record")? as usize;
    let usa_count = u16_at(rec, 6).ok_or("short record")? as usize;
    if usa_count == 0 || (usa_count - 1) * FIXUP_STRIDE > rec.len() {
        return Err("bad update sequence");
    }
    let check = u16_at(rec, usa_off).ok_or("bad update sequence")?;
    for i in 1..usa_count {
        let saved = u16_at(rec, usa_off + 2 * i).ok_or("bad update sequence")?;
        let end = i * FIXUP_STRIDE - 2;
        if u16_at(rec, end) != Some(check) {
            return Err("torn record");
        }
        rec[end..end + 2].copy_from_slice(&saved.to_le_bytes());
    }
    Ok(())
}

/// One attribute inside a record.
struct Attr<'a> {
    kind: u32,
    non_resident: bool,
    /// Attribute name length in UTF-16 units; 0 for the unnamed stream.
    name_len: u8,
    flags: u16,
    bytes: &'a [u8],
}

impl<'a> Attr<'a> {
    /// The value of a resident attribute.
    fn value(&self) -> Option<&'a [u8]> {
        if self.non_resident {
            return None;
        }
        let len = u32_at(self.bytes, 0x10)? as usize;
        let off = u16_at(self.bytes, 0x14)? as usize;
        self.bytes.get(off..off.checked_add(len)?)
    }
}

/// The attributes of a record that passed [`apply_fixups`].
fn attributes(rec: &[u8]) -> impl Iterator<Item = Attr<'_>> {
    let used = u32_at(rec, 0x18).map_or(0, |u| (u as usize).min(rec.len()));
    let mut off = u16_at(rec, 0x14).map_or(usize::MAX, usize::from);
    std::iter::from_fn(move || {
        let kind = u32_at(rec, off)?;
        if kind == ATTR_END {
            return None;
        }
        let len = u32_at(rec, off + 4)? as usize;
        if len < 0x18 || off + len > used {
            return None;
        }
        let bytes = &rec[off..off + len];
        off += len;
        Some(Attr {
            kind,
            non_resident: bytes[8] != 0,
            name_len: bytes[9],
            flags: u16_at(bytes, 0x0C)?,
            bytes,
        })
    })
}

/// A contiguous piece of a non-resident attribute: `clusters` clusters
/// starting at `lcn` on the volume, or a hole when `lcn` is `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    pub lcn: Option<u64>,
    pub clusters: u64,
}

/// Decode a mapping-pairs array: each run is a header byte (low nibble: size
/// of the length, high nibble: size of the signed offset from the previous
/// run's start), then the length and the offset, little-endian.
pub fn decode_runs(mut b: &[u8]) -> Option<Vec<Run>> {
    let mut runs = Vec::new();
    let mut lcn: i64 = 0;
    loop {
        let (&header, rest) = b.split_first()?;
        if header == 0 {
            return Some(runs);
        }
        let len_size = usize::from(header & 0x0F);
        let off_size = usize::from(header >> 4);
        if len_size == 0 || len_size > 8 || off_size > 8 || rest.len() < len_size + off_size {
            return None;
        }
        let mut clusters = 0u64;
        for (i, &byte) in rest[..len_size].iter().enumerate() {
            clusters |= u64::from(byte) << (8 * i);
        }
        let run_lcn = if off_size == 0 {
            None
        } else {
            let mut delta = 0i64;
            for (i, &byte) in rest[len_size..len_size + off_size].iter().enumerate() {
                delta |= i64::from(byte) << (8 * i);
            }
            // Sign-extend from the top bit of the last byte.
            let shift = 64 - 8 * off_size as u32;
            delta = (delta << shift) >> shift;
            lcn = lcn.checked_add(delta)?;
            Some(u64::try_from(lcn).ok()?)
        };
        runs.push(Run {
            lcn: run_lcn,
            clusters,
        });
        b = &rest[len_size + off_size..];
    }
}

/// The unnamed `$DATA` segment in a record: its first virtual cluster and
/// runs. Used to locate the MFT itself from record 0 and its extensions.
pub fn data_runs(rec: &[u8]) -> Option<(u64, Vec<Run>)> {
    let attr =
        attributes(rec).find(|a| a.kind == ATTR_DATA && a.name_len == 0 && a.non_resident)?;
    let lowest_vcn = u64_at(attr.bytes, 0x10)?;
    let pairs = u16_at(attr.bytes, 0x20)? as usize;
    Some((lowest_vcn, decode_runs(attr.bytes.get(pairs..)?)?))
}

/// Records holding further segments of the unnamed `$DATA` attribute, from a
/// resident `$ATTRIBUTE_LIST`. A very fragmented MFT describes itself this
/// way. The record itself is not included.
pub fn data_extension_records(rec: &[u8], own: u64) -> Vec<u64> {
    let Some(list) = attributes(rec)
        .find(|a| a.kind == ATTR_ATTRIBUTE_LIST)
        .and_then(|a| a.value())
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut off = 0usize;
    while let (Some(kind), Some(len)) = (u32_at(list, off), u16_at(list, off + 4)) {
        let len = usize::from(len);
        if len == 0 {
            break;
        }
        let name_len = list.get(off + 6).copied().unwrap_or(1);
        if kind == ATTR_DATA && name_len == 0 {
            if let Some(r) = u64_at(list, off + 0x10).map(record_of) {
                if r != own && !out.contains(&r) {
                    out.push(r);
                }
            }
        }
        off += len;
    }
    out
}

/// What one record contributes. An extension record (`base != number`)
/// carries attributes that did not fit in its base record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Info {
    number: u64,
    base: u64,
    sequence: u16,
    is_dir: bool,
    /// File attributes and last write time, from `$STANDARD_INFORMATION`.
    std: Option<(u32, i64)>,
    /// Allocated and logical size of the unnamed `$DATA` stream.
    data: Option<(u64, u64)>,
    reparse_tag: Option<u32>,
    /// Clusters used by named streams (alternate data streams), which hold
    /// real data: compressed system files keep theirs in one.
    stream_alloc: u64,
}

/// One name of a file: a hard link in folder `parent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NameRef {
    child: u32,
    parent: u32,
    parent_sequence: u16,
    namespace: u8,
    len: u8,
    /// Offset into the name arena.
    start: u32,
}

/// The records of one chunk of the MFT, parsed.
#[derive(Default)]
struct Chunk {
    infos: Vec<Info>,
    names: Vec<NameRef>,
    arena: Vec<u16>,
}

/// Parse a record that passed [`apply_fixups`] into `out`. Records not in
/// use are skipped.
fn parse_record(number: u64, rec: &[u8], out: &mut Chunk) {
    let Some(flags) = u16_at(rec, 0x16) else {
        return;
    };
    if flags & RECORD_IN_USE == 0 || number > u64::from(u32::MAX) {
        return;
    }
    let base = u64_at(rec, 0x20).map_or(0, record_of);
    let base = if base == 0 { number } else { base };
    let mut info = Info {
        number,
        base,
        sequence: u16_at(rec, 0x10).unwrap_or(0),
        is_dir: flags & RECORD_IS_DIRECTORY != 0,
        ..Info::default()
    };
    let mut name_tag = None;
    for attr in attributes(rec) {
        match attr.kind {
            ATTR_STANDARD_INFORMATION => {
                if let Some(v) = attr.value() {
                    if let (Some(modified), Some(attrs)) = (u64_at(v, 0x08), u32_at(v, 0x20)) {
                        info.std = Some((attrs, modified as i64));
                    }
                }
            }
            ATTR_FILE_NAME => {
                let Some(v) = attr.value() else { continue };
                let (Some(parent), Some(&len), Some(&namespace)) =
                    (u64_at(v, 0), v.get(0x40), v.get(0x41))
                else {
                    continue;
                };
                let Some(raw) = v.get(0x42..0x42 + 2 * usize::from(len)) else {
                    continue;
                };
                let parent_record = record_of(parent);
                if parent_record > u64::from(u32::MAX) {
                    continue;
                }
                if name_tag.is_none()
                    && u32_at(v, 0x38).unwrap_or(0) & FILE_ATTRIBUTE_REPARSE_POINT != 0
                {
                    name_tag = u32_at(v, 0x3C);
                }
                let start = out.arena.len() as u32;
                out.arena.extend(
                    raw.chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]])),
                );
                out.names.push(NameRef {
                    child: base as u32,
                    parent: parent_record as u32,
                    parent_sequence: sequence_of(parent),
                    namespace,
                    len,
                    start,
                });
            }
            ATTR_DATA if attr.name_len == 0 => {
                info.data = data_size(&attr);
            }
            // Resident named streams live inside the record.
            ATTR_DATA if attr.non_resident => {
                info.stream_alloc += data_size(&attr).map_or(0, |(alloc, _)| alloc);
            }
            ATTR_REPARSE_POINT => {
                info.reparse_tag = attr.value().and_then(|v| u32_at(v, 0));
            }
            _ => {}
        }
    }
    // A large reparse point is stored outside the record; `$FILE_NAME` keeps
    // a copy of its tag.
    if info.reparse_tag.is_none() {
        info.reparse_tag = name_tag;
    }
    out.infos.push(info);
}

/// Allocated and logical size of a `$DATA` attribute, if this segment holds
/// them (only the segment starting at virtual cluster 0 does).
fn data_size(attr: &Attr<'_>) -> Option<(u64, u64)> {
    if !attr.non_resident {
        let len = u64::from(u32_at(attr.bytes, 0x10)?);
        // Stored inside the record. Windows reports it rounded to 8 bytes.
        return Some((len.next_multiple_of(8), len));
    }
    if u64_at(attr.bytes, 0x10)? != 0 {
        return None;
    }
    let allocated = u64_at(attr.bytes, 0x28)?;
    let size = u64_at(attr.bytes, 0x30)?;
    // Compressed and sparse attributes also record the clusters actually
    // in use.
    let on_disk = if attr.flags & (ATTR_FLAG_COMPRESSED | ATTR_FLAG_SPARSE) != 0 {
        u64_at(attr.bytes, 0x40).unwrap_or(allocated)
    } else {
        allocated
    };
    Some((on_disk, size))
}

/// Per-record state after merging extension records.
#[derive(Debug, Clone, Copy, Default)]
struct Node {
    in_use: bool,
    is_dir: bool,
    sequence: u16,
    /// Names that count as hard links (DOS short names excluded).
    links: u16,
    attrs: u32,
    reparse_tag: u32,
    modified: i64,
    alloc: u64,
    size: u64,
    stream_alloc: u64,
}

/// Collects parsed records chunk by chunk, in any order.
pub struct Builder {
    nodes: Vec<Node>,
    names: Vec<NameRef>,
    arena: Vec<u16>,
}

impl Builder {
    pub fn new(records: u64) -> Self {
        Self {
            nodes: vec![Node::default(); usize::try_from(records).unwrap_or(0)],
            names: Vec::new(),
            arena: Vec::new(),
        }
    }

    /// Fix up and parse `chunk`, which holds consecutive records of
    /// `record_size` bytes starting at record `first`. Torn or unused
    /// records are skipped.
    pub fn add_chunk(&mut self, first: u64, chunk: &mut [u8], record_size: usize) {
        if record_size == 0 {
            return;
        }
        // Parsing is CPU work; split it across the scan's threads.
        const PER_TASK: usize = 1024;
        let parts: Vec<Chunk> = chunk
            .par_chunks_mut(record_size * PER_TASK)
            .enumerate()
            .map(|(task, part)| {
                let mut out = Chunk::default();
                for (i, rec) in part.chunks_exact_mut(record_size).enumerate() {
                    if apply_fixups(rec).is_ok() {
                        let number = first + (task * PER_TASK + i) as u64;
                        parse_record(number, rec, &mut out);
                    }
                }
                out
            })
            .collect();
        for part in parts {
            self.merge(part);
        }
    }

    fn merge(&mut self, chunk: Chunk) {
        let shift = self.arena.len() as u32;
        self.arena.extend_from_slice(&chunk.arena);
        self.names.extend(chunk.names.into_iter().map(|mut n| {
            n.start += shift;
            n
        }));
        for info in chunk.infos {
            let Some(node) = usize::try_from(info.base)
                .ok()
                .and_then(|i| self.nodes.get_mut(i))
            else {
                continue;
            };
            if info.base == info.number {
                node.in_use = true;
                node.is_dir = info.is_dir;
                node.sequence = info.sequence;
                if let Some((attrs, modified)) = info.std {
                    node.attrs = attrs;
                    node.modified = modified;
                }
            }
            if let Some((alloc, size)) = info.data {
                node.alloc = alloc;
                node.size = size;
            }
            node.stream_alloc += info.stream_alloc;
            if let Some(tag) = info.reparse_tag {
                node.reparse_tag = tag;
            }
        }
    }

    /// Link names to folders. Names whose folder was deleted and reused, and
    /// DOS 8.3 names of files that also have a long name, are dropped.
    pub fn finish(mut self) -> Tree {
        let n = self.nodes.len();
        let nodes = &self.nodes;
        self.names.retain(|name| {
            let (Some(child), Some(parent)) = (
                nodes.get(name.child as usize),
                nodes.get(name.parent as usize),
            ) else {
                return false;
            };
            child.in_use
                && parent.in_use
                && parent.is_dir
                && u64::from(name.child) >= FIRST_USER_RECORD
                && name.child != name.parent
                && (name.parent_sequence == 0 || name.parent_sequence == parent.sequence)
        });
        let mut has_long = vec![false; n];
        for name in &self.names {
            if name.namespace != NAMESPACE_DOS {
                has_long[name.child as usize] = true;
            }
        }
        self.names
            .retain(|name| name.namespace != NAMESPACE_DOS || !has_long[name.child as usize]);
        for name in &self.names {
            let node = &mut self.nodes[name.child as usize];
            node.links = node.links.saturating_add(1);
        }
        // Group names by folder (a counting sort keeps it linear).
        let mut offsets = vec![0u32; n + 1];
        for name in &self.names {
            offsets[name.parent as usize + 1] += 1;
        }
        for i in 0..n {
            offsets[i + 1] += offsets[i];
        }
        let mut next = offsets.clone();
        let mut sorted = vec![
            NameRef {
                child: 0,
                parent: 0,
                parent_sequence: 0,
                namespace: 0,
                len: 0,
                start: 0,
            };
            self.names.len()
        ];
        for name in self.names {
            let slot = &mut next[name.parent as usize];
            sorted[*slot as usize] = name;
            *slot += 1;
        }
        Tree {
            nodes: self.nodes,
            names: sorted,
            offsets,
            arena: self.arena,
        }
    }
}

/// The folder tree of a whole volume, built from its MFT.
pub struct Tree {
    nodes: Vec<Node>,
    /// Names grouped by folder: folder `d` holds `names[offsets[d]..offsets[d + 1]]`.
    names: Vec<NameRef>,
    offsets: Vec<u32>,
    arena: Vec<u16>,
}

#[cfg(windows)]
fn os_name(units: &[u16]) -> OsString {
    use std::os::windows::ffi::OsStringExt;
    OsString::from_wide(units)
}

#[cfg(not(windows))]
fn os_name(units: &[u16]) -> OsString {
    String::from_utf16_lossy(units).into()
}

impl Tree {
    /// Whether `record` is a folder in this tree.
    pub fn is_dir(&self, record: u64) -> bool {
        usize::try_from(record)
            .ok()
            .and_then(|i| self.nodes.get(i))
            .is_some_and(|n| n.in_use && n.is_dir)
    }

    /// The entries of folder `record`, as the directory listing would
    /// report them. `ino` is the record number, which identifies hard links
    /// and is the folder id for the next call.
    pub fn list(&self, record: u64) -> io::Result<Listing> {
        if !self.is_dir(record) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "folder not found in the file table",
            ));
        }
        let d = record as usize;
        let names = &self.names[self.offsets[d] as usize..self.offsets[d + 1] as usize];
        let entries = names
            .iter()
            .map(|name| {
                let node = &self.nodes[name.child as usize];
                let start = name.start as usize;
                let units = &self.arena[start..start + usize::from(name.len)];
                let attrs = node.attrs
                    | if node.is_dir {
                        FILE_ATTRIBUTE_DIRECTORY
                    } else {
                        0
                    };
                let kind = listing::kind_of(attrs, node.reparse_tag);
                let is_dir = kind == Kind::Dir;
                let stat = Stat {
                    kind,
                    alloc: if is_dir {
                        0
                    } else {
                        node.alloc + node.stream_alloc
                    },
                    logical: if is_dir { 0 } else { node.size },
                    modified_ms: listing::filetime_ms(node.modified),
                    nlink: u64::from(node.links.max(1)),
                    ino: u64::from(name.child),
                    other_volume: false,
                    // The table lists a folder's children directly.
                    empty_dir: false,
                };
                (os_name(units), stat)
            })
            .collect();
        Ok(Listing {
            entries,
            errors: Vec::new(),
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Builds FILE records the way NTFS writes them, fixups included.
    pub(crate) struct RecordBuilder {
        attrs: Vec<u8>,
        flags: u16,
        base: u64,
        sequence: u16,
    }

    pub(crate) const RECORD_SIZE: usize = 1024;
    /// 2020-01-01T00:00:00Z as a FILETIME.
    pub(crate) const JAN_2020: i64 = 132_223_104_000_000_000;

    impl RecordBuilder {
        pub fn file() -> Self {
            Self {
                attrs: Vec::new(),
                flags: RECORD_IN_USE,
                base: 0,
                sequence: 1,
            }
        }
        pub fn dir() -> Self {
            let mut b = Self::file();
            b.flags |= RECORD_IS_DIRECTORY;
            b
        }
        pub fn extension_of(mut self, base: u64) -> Self {
            self.base = base | (1 << 48);
            self
        }
        pub fn unused(mut self) -> Self {
            self.flags &= !RECORD_IN_USE;
            self
        }

        fn attr(mut self, kind: u32, non_resident: bool, flags: u16, body: &[u8]) -> Self {
            let header = if non_resident { 0x40 } else { 0x18 };
            let mut a = vec![0u8; header];
            a[0..4].copy_from_slice(&kind.to_le_bytes());
            a[8] = u8::from(non_resident);
            a[0x0C..0x0E].copy_from_slice(&flags.to_le_bytes());
            if !non_resident {
                a[0x10..0x14].copy_from_slice(&(body.len() as u32).to_le_bytes());
                a[0x14..0x16].copy_from_slice(&(header as u16).to_le_bytes());
                a[0x0A..0x0C].copy_from_slice(&(header as u16).to_le_bytes());
            }
            a.extend_from_slice(body);
            while a.len() % 8 != 0 {
                a.push(0);
            }
            let len = a.len() as u32;
            a[4..8].copy_from_slice(&len.to_le_bytes());
            self.attrs.extend_from_slice(&a);
            self
        }

        pub fn std_info(self, attrs: u32, modified: i64) -> Self {
            let mut v = vec![0u8; 0x48];
            v[0x08..0x10].copy_from_slice(&modified.to_le_bytes());
            v[0x20..0x24].copy_from_slice(&attrs.to_le_bytes());
            self.attr(ATTR_STANDARD_INFORMATION, false, 0, &v)
        }

        pub fn name(self, parent: u64, namespace: u8, name: &str) -> Self {
            let units: Vec<u16> = name.encode_utf16().collect();
            let mut v = vec![0u8; 0x42];
            // Parent sequence 1, as RecordBuilder writes it.
            v[0..8].copy_from_slice(&(parent | (1 << 48)).to_le_bytes());
            v[0x40] = units.len() as u8;
            v[0x41] = namespace;
            for u in units {
                v.extend_from_slice(&u.to_le_bytes());
            }
            self.attr(ATTR_FILE_NAME, false, 0, &v)
        }

        pub fn resident_data(self, len: usize) -> Self {
            self.attr(ATTR_DATA, false, 0, &vec![0xAB; len])
        }

        /// Non-resident unnamed data. `lowest_vcn` other than 0 makes a
        /// continuation segment, which holds no sizes.
        pub fn data(self, lowest_vcn: u64, allocated: u64, size: u64, runs: &[u8]) -> Self {
            let mut v = vec![0u8; 0x48 - 0x40];
            // Body after the 0x40-byte header: compressed size slot, then
            // the mapping pairs.
            v.extend_from_slice(runs);
            let mut b = self.attr(ATTR_DATA, true, 0, &v);
            let at = b.attrs.len() - {
                let mut l = 0x40 + v.len();
                while l % 8 != 0 {
                    l += 1;
                }
                l
            };
            let a = &mut b.attrs[at..];
            a[0x10..0x18].copy_from_slice(&lowest_vcn.to_le_bytes());
            a[0x20..0x22].copy_from_slice(&0x48u16.to_le_bytes());
            a[0x28..0x30].copy_from_slice(&allocated.to_le_bytes());
            a[0x30..0x38].copy_from_slice(&size.to_le_bytes());
            a[0x38..0x40].copy_from_slice(&size.to_le_bytes());
            b
        }

        /// A non-resident named stream.
        pub fn named_data(self, name: &str, allocated: u64, size: u64) -> Self {
            let mut b = self.data(0, allocated, size, &[0x11, 0x01, 0x40, 0x00]);
            // Mark the attribute just added as named; the name itself is
            // never read.
            let mut off = 0;
            let mut last = 0;
            while off < b.attrs.len() {
                last = off;
                off += u32::from_le_bytes(b.attrs[off + 4..off + 8].try_into().unwrap()) as usize;
            }
            b.attrs[last + 9] = name.encode_utf16().count() as u8;
            b
        }

        pub fn reparse(self, tag: u32) -> Self {
            let mut v = tag.to_le_bytes().to_vec();
            v.extend_from_slice(&[0u8; 4]);
            self.attr(ATTR_REPARSE_POINT, false, 0, &v)
        }

        pub fn attribute_list(self, entries: &[(u32, u64)]) -> Self {
            let mut v = Vec::new();
            for &(kind, record) in entries {
                let mut e = vec![0u8; 0x20];
                e[0..4].copy_from_slice(&kind.to_le_bytes());
                e[4..6].copy_from_slice(&0x20u16.to_le_bytes());
                e[0x10..0x18].copy_from_slice(&(record | (1 << 48)).to_le_bytes());
                v.extend_from_slice(&e);
            }
            self.attr(ATTR_ATTRIBUTE_LIST, false, 0, &v)
        }

        /// The record as stored on disk, protected by an update sequence.
        pub fn build(self) -> Vec<u8> {
            let mut r = vec![0u8; RECORD_SIZE];
            r[0..4].copy_from_slice(b"FILE");
            let usa_off = 0x30usize;
            let usa_count = RECORD_SIZE / FIXUP_STRIDE + 1;
            r[4..6].copy_from_slice(&(usa_off as u16).to_le_bytes());
            r[6..8].copy_from_slice(&(usa_count as u16).to_le_bytes());
            r[0x10..0x12].copy_from_slice(&self.sequence.to_le_bytes());
            let first = (usa_off + 2 * usa_count).next_multiple_of(8);
            r[0x14..0x16].copy_from_slice(&(first as u16).to_le_bytes());
            r[0x16..0x18].copy_from_slice(&self.flags.to_le_bytes());
            r[0x20..0x28].copy_from_slice(&self.base.to_le_bytes());
            r[first..first + self.attrs.len()].copy_from_slice(&self.attrs);
            let end = first + self.attrs.len();
            r[end..end + 4].copy_from_slice(&ATTR_END.to_le_bytes());
            let used = (end + 8) as u32;
            r[0x18..0x1C].copy_from_slice(&used.to_le_bytes());
            r[0x1C..0x20].copy_from_slice(&(RECORD_SIZE as u32).to_le_bytes());
            // Protect each stride: save its last two bytes, write the check.
            let check: u16 = 0x0042;
            r[usa_off..usa_off + 2].copy_from_slice(&check.to_le_bytes());
            for i in 1..usa_count {
                let end = i * FIXUP_STRIDE - 2;
                let saved = [r[end], r[end + 1]];
                r[usa_off + 2 * i..usa_off + 2 * i + 2].copy_from_slice(&saved);
                r[end..end + 2].copy_from_slice(&check.to_le_bytes());
            }
            r
        }
    }

    /// An MFT image: `records[i]` is record `i`; missing ones are empty.
    pub(crate) fn image(records: &[(u64, Vec<u8>)]) -> (Vec<u8>, u64) {
        let count = records.iter().map(|(n, _)| n + 1).max().unwrap_or(0);
        let mut img = vec![0u8; count as usize * RECORD_SIZE];
        for (n, r) in records {
            let at = *n as usize * RECORD_SIZE;
            img[at..at + RECORD_SIZE].copy_from_slice(r);
        }
        (img, count)
    }

    pub(crate) fn tree(records: &[(u64, Vec<u8>)]) -> Tree {
        let (mut img, count) = image(records);
        let mut b = Builder::new(count);
        // Two chunks, to cover merging across chunk boundaries.
        let split = (count as usize / 2) * RECORD_SIZE;
        let (a, rest) = img.split_at_mut(split);
        b.add_chunk((split / RECORD_SIZE) as u64, rest, RECORD_SIZE);
        b.add_chunk(0, a, RECORD_SIZE);
        b.finish()
    }

    fn entries(t: &Tree, dir: u64) -> Vec<(String, Stat)> {
        let mut v: Vec<(String, Stat)> = t
            .list(dir)
            .unwrap()
            .entries
            .into_iter()
            .map(|(n, s)| (n.to_string_lossy().into_owned(), s))
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }

    #[test]
    fn fixups_restore_sector_ends_and_catch_torn_writes() {
        let mut r = RecordBuilder::file()
            .std_info(0x20, JAN_2020)
            .name(5, 1, "x")
            .build();
        let pristine = r.clone();
        assert_eq!(&r[510..512], &[0x42, 0]);
        apply_fixups(&mut r).unwrap();
        assert_ne!(r, pristine);
        assert_eq!(&r[510..512], &[0, 0]);

        let mut torn = pristine.clone();
        torn[1022] = 0x99;
        assert_eq!(apply_fixups(&mut torn), Err("torn record"));
        let mut other = pristine;
        other[0] = b'B';
        assert!(apply_fixups(&mut other).is_err());
    }

    #[test]
    fn mapping_pairs_decode() {
        // 0x21: 1 length byte, 2 offset bytes. Run 1: 0x18 clusters at
        // 0x5634. Run 2: 0x10 clusters, offset -0x10 (0xFFF0) back to
        // 0x5624. Run 3: a 4-cluster hole. Then the end marker.
        let runs = decode_runs(&[
            0x21, 0x18, 0x34, 0x56, 0x21, 0x10, 0xF0, 0xFF, 0x01, 0x04, 0x00,
        ])
        .unwrap();
        assert_eq!(
            runs,
            vec![
                Run {
                    lcn: Some(0x5634),
                    clusters: 0x18
                },
                Run {
                    lcn: Some(0x5624),
                    clusters: 0x10
                },
                Run {
                    lcn: None,
                    clusters: 4
                },
            ]
        );
        assert_eq!(decode_runs(&[0x21, 0x18]), None);
    }

    #[test]
    fn mft_runs_and_attribute_list() {
        let mut r = RecordBuilder::file()
            .std_info(0x06, JAN_2020)
            .attribute_list(&[
                (ATTR_STANDARD_INFORMATION, 0),
                (ATTR_DATA, 0),
                (ATTR_DATA, 27),
            ])
            .name(5, 3, "$MFT")
            .data(0, 0x40_000, 0x40_000, &[0x31, 0x40, 0x00, 0x00, 0x0C, 0x00])
            .build();
        apply_fixups(&mut r).unwrap();
        let (vcn, runs) = data_runs(&r).unwrap();
        assert_eq!(vcn, 0);
        assert_eq!(
            runs,
            vec![Run {
                lcn: Some(0x0C_0000),
                clusters: 0x40
            }]
        );
        assert_eq!(data_extension_records(&r, 0), vec![27]);
    }

    #[test]
    fn records_build_a_tree() {
        let tag_symlink = 0xA000_000C;
        let tag_cloud = 0x9000_001A;
        let t = tree(&[
            (
                5,
                RecordBuilder::dir()
                    .std_info(0x16, JAN_2020)
                    .name(5, 3, ".")
                    .build(),
            ),
            // NTFS metadata is never listed.
            (
                6,
                RecordBuilder::file()
                    .std_info(0x06, JAN_2020)
                    .name(5, 3, "$Bitmap")
                    .build(),
            ),
            (
                16,
                RecordBuilder::dir()
                    .std_info(0x10, JAN_2020)
                    .name(5, 1, "Program Files")
                    .name(5, 2, "PROGRA~1")
                    .build(),
            ),
            (
                17,
                RecordBuilder::file()
                    .std_info(0x20, JAN_2020)
                    .name(16, 1, "big.bin")
                    .name(16, 2, "BIG~1.BIN")
                    .data(0, 1 << 20, 1_000_000, &[0x21, 0x01, 0x00, 0x10, 0x00])
                    .build(),
            ),
            // A hard link: two long names in different folders.
            (
                18,
                RecordBuilder::file()
                    .std_info(0x20, JAN_2020)
                    .name(5, 0, "posix-name")
                    .name(16, 1, "link")
                    .resident_data(13)
                    .build(),
            ),
            (
                19,
                RecordBuilder::dir()
                    .std_info(0x410, JAN_2020)
                    .name(5, 1, "junction")
                    .reparse(tag_symlink)
                    .build(),
            ),
            (
                20,
                RecordBuilder::file()
                    .std_info(0x420, JAN_2020)
                    .name(5, 1, "cloud.docx")
                    .reparse(tag_cloud)
                    .data(0, 0, 50_000, &[0x00])
                    .build(),
            ),
            // A file whose data did not fit: sizes come from its extension.
            (
                21,
                RecordBuilder::file()
                    .std_info(0x20, JAN_2020)
                    .name(5, 1, "frag.bin")
                    .build(),
            ),
            (
                22,
                RecordBuilder::file()
                    .extension_of(21)
                    .data(0, 8192, 8000, &[0x11, 0x02, 0x30, 0x00])
                    .named_data("WofCompressedData", 4096, 3000)
                    .build(),
            ),
            (
                23,
                RecordBuilder::file().unused().name(5, 1, "deleted").build(),
            ),
            // A name in a folder that was deleted.
            (
                24,
                RecordBuilder::file()
                    .std_info(0x20, JAN_2020)
                    .name(23, 1, "orphan")
                    .build(),
            ),
        ]);
        let root = entries(&t, ROOT_RECORD);
        let names: Vec<&str> = root.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "Program Files",
                "cloud.docx",
                "frag.bin",
                "junction",
                "posix-name"
            ]
        );
        let get = |v: &[(String, Stat)], n: &str| v.iter().find(|(x, _)| x == n).unwrap().1;
        let pf = get(&root, "Program Files");
        assert_eq!(pf.kind, Kind::Dir);
        assert_eq!(pf.ino, 16);
        assert_eq!(get(&root, "junction").kind, Kind::Symlink);
        let cloud = get(&root, "cloud.docx");
        assert_eq!(
            (cloud.kind, cloud.alloc, cloud.logical),
            (Kind::File, 0, 50_000)
        );
        let frag = get(&root, "frag.bin");
        // The named stream's clusters count; its length does not.
        assert_eq!((frag.alloc, frag.logical), (8192 + 4096, 8000));
        let posix = get(&root, "posix-name");
        assert_eq!((posix.alloc, posix.logical, posix.nlink), (16, 13, 2));
        assert_eq!(posix.modified_ms, Some(1_577_836_800_000));

        let inside = entries(&t, 16);
        let names: Vec<&str> = inside.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["big.bin", "link"]);
        let big = get(&inside, "big.bin");
        assert_eq!((big.alloc, big.logical, big.nlink), (1 << 20, 1_000_000, 1));
        assert_eq!(get(&inside, "link").ino, 18);

        assert!(t.list(17).is_err());
        assert!(t.list(19).is_ok_and(|l| l.entries.is_empty()));
        assert!(t.list(9999).is_err());
    }
}
