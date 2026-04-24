use std::alloc::Layout;
use std::mem;
use std::ops::Range;
use std::ptr::{self, NonNull};

use crate::error::LinkError;

/// Header entry for one opcode's metadata block inside a linked artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct MetadataHeaderEntry {
    /// Byte offset of the first metadata slot for the opcode.
    pub offset: u32,
    /// Number of metadata slots allocated for the opcode.
    pub len: u32,
}

/// Type information needed to allocate, access, and drop linked metadata.
#[derive(Debug, Clone, Copy)]
pub struct MetadataTypeSpec {
    /// Human-readable type name for diagnostics and reflection.
    pub type_name: &'static str,
    /// `size_of::<T>()` for the metadata payload type.
    pub size: usize,
    /// `align_of::<T>()` for the metadata payload type.
    pub align: usize,
    /// Drop glue used when a linked artifact is destroyed.
    pub drop_fn: unsafe fn(*mut u8),
}

impl MetadataTypeSpec {
    /// Creates a metadata type spec for `T`.

    pub fn of<T>(type_name: &'static str) -> Self {
        Self {
            type_name,
            size: mem::size_of::<T>(),
            align: mem::align_of::<T>(),
            drop_fn: drop_value::<T>,
        }
    }
}

unsafe fn drop_value<T>(ptr: *mut u8) {
    unsafe {
        ptr::drop_in_place(ptr.cast::<T>());
    }
}

/// Planned layout for the metadata table stored in a linked artifact.
#[derive(Debug, Clone)]
pub struct MetadataTableLayout {
    /// Total allocation layout for the header array and payload blocks.
    pub layout: Layout,
    /// One header per opcode, indexed by opcode id.
    pub headers: Box<[MetadataHeaderEntry]>,
}

impl MetadataTableLayout {
    /// Returns the header for `opcode`, if one exists.

    pub fn header(&self, opcode: usize) -> Option<&MetadataHeaderEntry> {
        self.headers.get(opcode)
    }
}

pub(crate) fn metadata_table_header_len(num_opcodes: usize) -> Result<usize, LinkError> {
    let offset16_len =
        mem::size_of::<u16>()
            .checked_mul(num_opcodes)
            .ok_or(LinkError::LayoutOverflow(
                "metadata u16 table size overflow",
            ))?;
    let offset32_len =
        mem::size_of::<u32>()
            .checked_mul(num_opcodes)
            .ok_or(LinkError::LayoutOverflow(
                "metadata u32 table size overflow",
            ))?;
    offset16_len
        .checked_add(offset32_len)
        .ok_or(LinkError::LayoutOverflow(
            "metadata table header size overflow",
        ))
}

pub(crate) fn metadata_offset(bytes: &[u8], opcode: usize, num_opcodes: usize) -> Option<u32> {
    if opcode >= num_opcodes {
        return None;
    }

    let offset16_start = opcode.checked_mul(mem::size_of::<u16>())?;
    let offset16_end = offset16_start.checked_add(mem::size_of::<u16>())?;
    let raw16 = bytes.get(offset16_start..offset16_end)?;
    let raw16 = u16::from_ne_bytes([raw16[0], raw16[1]]);
    if raw16 != 0 {
        return Some(u32::from(raw16));
    }

    let offset32_base = mem::size_of::<u16>().checked_mul(num_opcodes)?;
    let offset32_start = offset32_base.checked_add(opcode.checked_mul(mem::size_of::<u32>())?)?;
    let offset32_end = offset32_start.checked_add(mem::size_of::<u32>())?;
    let raw32 = bytes.get(offset32_start..offset32_end)?;
    Some(u32::from_ne_bytes([raw32[0], raw32[1], raw32[2], raw32[3]]))
}

pub(crate) fn write_metadata_offset(bytes: &mut [u8], opcode: usize, num_opcodes: usize, offset: u32) {
    let offset16_start = opcode * mem::size_of::<u16>();
    let offset16_end = offset16_start + mem::size_of::<u16>();
    if offset != 0 && u16::try_from(offset).is_ok() {
        bytes[offset16_start..offset16_end].copy_from_slice(&(offset as u16).to_ne_bytes());
        return;
    }

    bytes[offset16_start..offset16_end].copy_from_slice(&0u16.to_ne_bytes());
    let offset32_base = mem::size_of::<u16>() * num_opcodes;
    let offset32_start = offset32_base + (opcode * mem::size_of::<u32>());
    let offset32_end = offset32_start + mem::size_of::<u32>();
    bytes[offset32_start..offset32_end].copy_from_slice(&offset.to_ne_bytes());
}

#[inline(always)]
pub(crate) fn debug_assert_metadata_type_matches<T>(spec: &MetadataTypeSpec) {
    debug_assert_eq!(
        spec.size,
        mem::size_of::<T>(),
        "metadata type mismatch: stored {} has size {}, requested {} has size {}",
        spec.type_name,
        spec.size,
        std::any::type_name::<T>(),
        mem::size_of::<T>(),
    );
    debug_assert_eq!(
        spec.align,
        mem::align_of::<T>(),
        "metadata type mismatch: stored {} has align {}, requested {} has align {}",
        spec.type_name,
        spec.align,
        std::any::type_name::<T>(),
        mem::align_of::<T>(),
    );
}

pub(crate) fn metadata_slot_range(
    spec: &MetadataTypeSpec,
    header: MetadataHeaderEntry,
    slot: u32,
) -> Option<Range<usize>> {
    let start = (header.offset as usize).checked_add(spec.size.checked_mul(slot as usize)?)?;
    let end = start.checked_add(spec.size)?;
    Some(start..end)
}

/// Plans a packed metadata table for a set of per-opcode slot counts and types.
pub fn plan_metadata_table(
    counts: &[u32],
    types: &[MetadataTypeSpec],
) -> Result<MetadataTableLayout, LinkError> {
    if counts.len() != types.len() {
        return Err(LinkError::LayoutOverflow(
            "metadata count/spec length mismatch",
        ));
    }

    let header_size = metadata_table_header_len(types.len())?;
    let mut offset = header_size;
    let mut headers = Vec::with_capacity(types.len());

    for (index, (spec, count)) in types.iter().copied().zip(counts.iter().copied()).enumerate() {
        if count == 0 {
            headers.push(MetadataHeaderEntry { offset: 0, len: 0 });
            continue;
        }

        offset = align_up(offset, spec.align.max(1))?;
        let offset_u32 = u32::try_from(offset).map_err(|_| {
            LinkError::LayoutOverflow("metadata offset overflowed u32 during planning")
        })?;
        headers.push(MetadataHeaderEntry {
            offset: offset_u32,
            len: count,
        });
        let slot_total = spec
            .size
            .checked_mul(count as usize)
            .ok_or(LinkError::LayoutOverflow("metadata slot total size overflow"))?;
        offset = offset
            .checked_add(slot_total)
            .ok_or(LinkError::LayoutOverflow("metadata total size overflow"))?;
        let _ = index; // suppress unused warning in release
    }

    let alignment = types
        .iter()
        .map(|spec| spec.align.max(1))
        .max()
        .unwrap_or(1);
    let total = align_up(offset, alignment)?;
    let layout = Layout::from_size_align(total, alignment)
        .map_err(|_| LinkError::LayoutOverflow("metadata layout computation failed"))?;

    Ok(MetadataTableLayout {
        layout,
        headers: headers.into_boxed_slice(),
    })
}

/// Zero-initializes a metadata allocation and writes the offset tables at the front.
pub fn initialize_metadata_table(bytes: &mut [u8], headers: &[MetadataHeaderEntry]) {
    bytes.fill(0);
    if headers.is_empty() {
        return;
    }
    for (opcode, header) in headers.iter().copied().enumerate() {
        write_metadata_offset(bytes, opcode, headers.len(), header.offset);
    }
}

/// Returns a typed pointer to one metadata slot in an initialized metadata table.
///
/// # Safety
///
/// `bytes` must refer to a metadata allocation compatible with `header`, and `slot` must be in
/// bounds for the associated opcode table. The caller is responsible for initializing the slot
/// before reading from the returned pointer.
pub unsafe fn metadata_slot_ptr<T>(
    bytes: &mut [u8],
    header: MetadataHeaderEntry,
    slot: u32,
) -> *mut T {
    if mem::size_of::<T>() == 0 {
        NonNull::<T>::dangling().as_ptr()
    } else {
        unsafe {
            bytes
                .as_mut_ptr()
                .add(header.offset as usize + mem::size_of::<T>() * slot as usize)
                .cast::<T>()
        }
    }
}

fn align_up(value: usize, align: usize) -> Result<usize, LinkError> {
    let mask = align
        .checked_sub(1)
        .ok_or(LinkError::LayoutOverflow("invalid alignment"))?;
    value
        .checked_add(mask)
        .map(|aligned| aligned & !mask)
        .ok_or(LinkError::LayoutOverflow("layout alignment overflow"))
}
