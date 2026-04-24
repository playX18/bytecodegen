use std::ptr::NonNull;
use std::slice;


use super::planning::{
    MetadataHeaderEntry, MetadataTypeSpec, debug_assert_metadata_type_matches, metadata_offset,
    metadata_slot_range,
};

/// Shared view over the metadata table of a linked artifact.
pub struct MetadataTableView<'a> {
    bytes: &'a [u8],
    types: &'a [MetadataTypeSpec],
    counts: &'a [u32],
}

impl<'a> MetadataTableView<'a> {
    /// Creates a view over raw metadata bytes and their corresponding type specs.

    pub fn new(bytes: &'a [u8], types: &'a [MetadataTypeSpec], counts: &'a [u32]) -> Self {
        Self {
            bytes,
            types,
            counts,
        }
    }

    /// Returns the number of opcode metadata tables described by this view.

    pub fn num_opcodes(&self) -> usize {
        self.types.len().min(self.counts.len())
    }

    /// Returns the header entry for `opcode`, if present.

    pub fn header(&self, opcode: usize) -> Option<MetadataHeaderEntry> {
        let len = *self.counts.get(opcode)?;
        let num_opcodes = self.num_opcodes();
        let offset = metadata_offset(self.bytes, opcode, num_opcodes)?;
        Some(MetadataHeaderEntry { offset, len })
    }

    /// Returns a typed reference to one metadata slot.
    pub fn get_typed<T>(&self, opcode: usize, slot: u32) -> Option<&'a T> {
        let spec = self.types.get(opcode)?;
        debug_assert_metadata_type_matches::<T>(spec);
        let header = self.header(opcode)?;
        if slot >= header.len {
            return None;
        }
        if spec.size == 0 {
            let ptr = NonNull::<T>::dangling().as_ptr();
            return Some(unsafe { &*ptr });
        }
        let bytes = self.bytes.get(metadata_slot_range(spec, header, slot)?)?;
        Some(unsafe { &*(bytes.as_ptr().cast::<T>()) })
    }
}

/// Mutable view over the metadata table of a linked artifact.
pub struct MetadataTableViewMut<'a> {
    bytes: &'a mut [u8],
    types: &'a [MetadataTypeSpec],
    counts: &'a [u32],
}

impl<'a> MetadataTableViewMut<'a> {
    /// Creates a mutable metadata-table view.

    pub fn new(bytes: &'a mut [u8], types: &'a [MetadataTypeSpec], counts: &'a [u32]) -> Self {
        Self {
            bytes,
            types,
            counts,
        }
    }

    /// Returns a mutable typed reference to one metadata slot.
    pub fn get_typed_mut<T>(&mut self, opcode: usize, slot: u32) -> Option<&mut T> {
        let spec = self.types.get(opcode)?;
        debug_assert_metadata_type_matches::<T>(spec);
        let header = MetadataTableView::new(self.bytes, self.types, self.counts).header(opcode)?;
        if slot >= header.len {
            return None;
        }
        if spec.size == 0 {
            let ptr = NonNull::<T>::dangling().as_ptr();
            return Some(unsafe { &mut *ptr });
        }
        let bytes = self
            .bytes
            .get_mut(metadata_slot_range(spec, header, slot)?)?;
        Some(unsafe { &mut *(bytes.as_mut_ptr().cast::<T>()) })
    }
}

/// Per-instruction index entry stored alongside linked bytecode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct LinkedInstructionEntry {
    /// Starting byte offset of the encoded instruction.
    pub offset: u32,
}

impl LinkedInstructionEntry {
    /// Creates an index entry for an instruction.

    pub const fn without_metadata(offset: u32) -> Self {
        Self { offset }
    }
}

/// Common interface for fully linked bytecode artifacts.
pub trait LinkedBytecodeView {
    /// Returns the encoded instruction stream.
    fn bytes(&self) -> &[u8];
    /// Returns the per-instruction ordinal table.
    fn instructions(&self) -> &[LinkedInstructionEntry];
    /// Returns the metadata type table indexed by opcode id.
    fn metadata_types(&self) -> &[MetadataTypeSpec];
    /// Returns the metadata slot counts indexed by opcode id.
    fn metadata_counts(&self) -> &[u32];
    /// Returns the raw metadata-table bytes.
    fn metadata_bytes(&self) -> &[u8];
    /// Returns the raw metadata-table bytes mutably.
    fn metadata_bytes_mut(&mut self) -> &mut [u8];

    /// Returns the number of linked instructions.

    fn instruction_count(&self) -> usize {
        self.instructions().len()
    }

    /// Returns a typed view over the metadata table.

    fn metadata_table(&self) -> MetadataTableView<'_> {
        MetadataTableView::new(
            self.metadata_bytes(),
            self.metadata_types(),
            self.metadata_counts(),
        )
    }

    /// Returns a typed reference to one metadata slot.
    fn metadata_typed<T>(&self, opcode: usize, slot: u32) -> Option<&T> {
        self.metadata_table().get_typed::<T>(opcode, slot)
    }

    fn __metadata_slices_mut(&mut self) -> (&mut [u8], &[MetadataTypeSpec], &[u32]) {
        let types_len = self.metadata_types().len();
        let types_ptr = self.metadata_types().as_ptr();
        let counts_len = self.metadata_counts().len();
        let counts_ptr = self.metadata_counts().as_ptr();
        let bytes = self.metadata_bytes_mut();
        let types = unsafe { slice::from_raw_parts(types_ptr, types_len) };
        let counts = unsafe { slice::from_raw_parts(counts_ptr, counts_len) };
        (bytes, types, counts)
    }

    /// Returns a mutable typed view over the metadata table.
    fn metadata_table_mut(&mut self) -> MetadataTableViewMut<'_> {
        let (bytes, types, counts) = self.__metadata_slices_mut();
        MetadataTableViewMut::new(bytes, types, counts)
    }

    /// Returns a mutable typed reference to one metadata slot.
    fn metadata_typed_mut<T>(&mut self, opcode: usize, slot: u32) -> Option<&mut T> {
        let (bytes, types, counts) = self.__metadata_slices_mut();
        let spec = types.get(opcode)?;
        let header = MetadataTableView::new(bytes, types, counts).header(opcode)?;
        if slot >= header.len {
            return None;
        }
        if spec.size == 0 {
            let ptr = NonNull::<T>::dangling().as_ptr();
            return Some(unsafe { &mut *ptr });
        }
        let start = (header.offset as usize).checked_add(spec.size.checked_mul(slot as usize)?)?;
        let end = start.checked_add(spec.size)?;
        let bytes = bytes.get_mut(start..end)?;
        Some(unsafe { &mut *(bytes.as_mut_ptr().cast::<T>()) })
    }
}
