use std::alloc::{self, Layout};
use std::mem;
use std::ops::Range;
use std::ptr::{self, NonNull};
use std::slice;

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

fn metadata_table_header_len(num_opcodes: usize) -> Result<usize, LinkError> {
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

fn metadata_offset(bytes: &[u8], opcode: usize, num_opcodes: usize) -> Option<u32> {
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

fn write_metadata_offset(bytes: &mut [u8], opcode: usize, num_opcodes: usize, offset: u32) {
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
fn debug_assert_metadata_type_matches<T>(spec: &MetadataTypeSpec) {
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

fn metadata_slot_range(
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
    let mut headers = Vec::with_capacity(types.len());
    let mut offset = header_size;
    let mut max_align = mem::align_of::<u32>();

    for spec in types {
        max_align = max_align.max(spec.align.max(1));
    }

    for (count, spec) in counts.iter().copied().zip(types.iter().copied()) {
        offset = align_up(offset, spec.align.max(1))?;
        let block_size = spec
            .size
            .checked_mul(count as usize)
            .ok_or(LinkError::LayoutOverflow("metadata payload size overflow"))?;
        headers.push(MetadataHeaderEntry {
            offset: offset
                .try_into()
                .map_err(|_| LinkError::LayoutOverflow("metadata payload offset exceeds u32"))?,
            len: count,
        });
        offset = offset
            .checked_add(block_size)
            .ok_or(LinkError::LayoutOverflow("metadata table size overflow"))?;
    }

    let layout = Layout::from_size_align(offset, max_align)
        .map_err(|_| LinkError::LayoutOverflow("invalid metadata table layout"))?;

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

/// Metadata needed by a [`LinkBackendV2`] to finalize its artifact.
#[derive(Debug, Clone)]
pub struct LinkedArtifactSpec {
    /// Metadata payload types indexed by opcode id.
    pub metadata_types: Box<[MetadataTypeSpec]>,
    /// Metadata slot counts indexed by opcode id.
    pub metadata_counts: Box<[u32]>,
    /// Planned metadata allocation layout.
    pub metadata_layout: Layout,
}

/// Borrowed placement returned by a [`LinkBackendV2`] allocation method.
#[derive(Debug, Clone, Copy)]
pub struct Placement<T> {
    ptr: NonNull<T>,
    len: usize,
}

impl<T> Placement<T> {
    /// Creates a placement from a raw pointer and length.

    pub const fn new(ptr: NonNull<T>, len: usize) -> Self {
        Self { ptr, len }
    }

    /// Returns the number of elements in the placement.

    pub const fn len(self) -> usize {
        self.len
    }

    /// Returns whether the placement contains no elements.

    pub const fn is_empty(self) -> bool {
        self.len == 0
    }

    /// Exposes the placement as a mutable slice.
    ///
    /// # Safety
    ///
    /// The backend must ensure the underlying allocation stays valid for the duration
    /// of the returned slice and that no aliased mutable access exists.
    pub unsafe fn as_mut_slice<'a>(&mut self) -> &'a mut [T] {
        unsafe { slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

/// Specification of what the linker needs to allocate.
#[derive(Debug, Clone)]
pub struct LinkSpec {
    /// Number of bytes needed for the instruction stream.
    pub code_size: usize,
    /// Number of instruction ordinal entries.
    pub instruction_count: usize,
    /// Memory layout for the metadata table allocation.
    pub metadata_layout: Layout,
}

/// Allocations provided by a [`LinkBackendV2`].
#[derive(Debug)]
pub struct LinkAllocations {
    /// Writable placement for the encoded instruction bytes.
    pub code: Placement<u8>,
    /// Writable placement for the instruction ordinal table.
    pub ordinals: Placement<LinkedInstructionEntry>,
    /// Writable placement for the metadata table.
    pub metadata: Placement<u8>,
}

/// Simplified link backend trait.
///
/// Implement [`allocate`](LinkBackendV2::allocate) to provide writable memory and
/// [`commit`](LinkBackendV2::commit) to finalize the linked artifact.
pub trait LinkBackendV2 {
    /// Final linked artifact type produced by the backend.
    type Bytecode;

    /// Allocates all buffers needed for linking according to `spec`.
    ///
    /// The caller writes bytecode, ordinals, and metadata into the returned
    /// placements, then passes them to [`commit`](LinkBackendV2::commit).
    fn allocate(&mut self, spec: &LinkSpec) -> Result<LinkAllocations, LinkError>;

    /// Takes the filled allocations and produces the final linked artifact.
    ///
    /// The backend transfers ownership of its internal buffers into the
    /// resulting bytecode; the `allocations` parameter is carried for trait
    /// compatibility but the backing memory is owned by `self`.
    fn commit(
        self,
        allocations: LinkAllocations,
        spec: LinkedArtifactSpec,
    ) -> Result<Self::Bytecode, LinkError>;
}

struct RawAllocation {
    ptr: NonNull<u8>,
    layout: Layout,
}

impl RawAllocation {
    fn new(layout: Layout) -> Result<Self, LinkError> {
        if layout.size() == 0 {
            return Ok(Self {
                ptr: NonNull::dangling(),
                layout,
            });
        }

        let ptr = unsafe { alloc::alloc(layout) };
        let ptr =
            NonNull::new(ptr).ok_or_else(|| LinkError::Backend("allocation failed".to_string()))?;
        Ok(Self { ptr, layout })
    }

    fn as_slice(&self) -> &[u8] {
        if self.layout.size() == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(self.ptr.as_ptr(), self.layout.size()) }
        }
    }

    fn as_slice_mut(&mut self) -> &mut [u8] {
        if self.layout.size() == 0 {
            &mut []
        } else {
            unsafe { slice::from_raw_parts_mut(self.ptr.as_ptr(), self.layout.size()) }
        }
    }
}

impl Drop for RawAllocation {
    fn drop(&mut self) {
        if self.layout.size() > 0 {
            unsafe {
                alloc::dealloc(self.ptr.as_ptr(), self.layout);
            }
        }
    }
}

/// [`LinkBackendV2`] implementation that allocates code and metadata on the native heap.
pub struct NativeHeapBackend {
    code: Option<Box<[u8]>>,
    metadata: Option<RawAllocation>,
    ordinals: Option<Box<[LinkedInstructionEntry]>>,
}

impl NativeHeapBackend {
    /// Creates an empty native-heap backend.

    pub fn new() -> Self {
        Self {
            code: None,
            metadata: None,
            ordinals: None,
        }
    }

    fn allocate_code(&mut self, len: usize) -> Result<Placement<u8>, LinkError> {
        self.code = Some(vec![0; len].into_boxed_slice());
        let code = self.code.as_mut().expect("code allocated");
        Ok(Placement::new(non_null_from_slice(code), code.len()))
    }

    fn allocate_metadata(&mut self, layout: Layout) -> Result<Placement<u8>, LinkError> {
        self.metadata = Some(RawAllocation::new(layout)?);
        let bytes = self
            .metadata
            .as_mut()
            .expect("metadata allocated")
            .as_slice_mut();
        bytes.fill(0);
        Ok(Placement::new(non_null_from_slice(bytes), bytes.len()))
    }

    fn allocate_ordinals(
        &mut self,
        len: usize,
    ) -> Result<Placement<LinkedInstructionEntry>, LinkError> {
        self.ordinals =
            Some(vec![LinkedInstructionEntry::without_metadata(0); len].into_boxed_slice());
        let ordinals = self.ordinals.as_mut().expect("ordinals allocated");
        Ok(Placement::new(
            non_null_from_slice(ordinals),
            ordinals.len(),
        ))
    }
}

impl Default for NativeHeapBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// Linked bytecode artifact produced by [`NativeHeapBackend`].
pub struct NativeLinkedBytecode {
    code: Box<[u8]>,
    metadata: RawAllocation,
    metadata_types: Box<[MetadataTypeSpec]>,
    metadata_counts: Box<[u32]>,
    ordinals: Box<[LinkedInstructionEntry]>,
}

impl Drop for NativeLinkedBytecode {
    fn drop(&mut self) {
        let table = MetadataTableView::new(
            self.metadata.as_slice(),
            &self.metadata_types,
            &self.metadata_counts,
        );
        for (opcode, spec) in self.metadata_types.iter().copied().enumerate() {
            let Some(header) = table.header(opcode) else {
                continue;
            };
            for slot in 0..header.len {
                let ptr = if spec.size == 0 {
                    NonNull::<u8>::dangling().as_ptr()
                } else {
                    unsafe {
                        self.metadata
                            .ptr
                            .as_ptr()
                            .add(header.offset as usize + spec.size * slot as usize)
                    }
                };
                unsafe { (spec.drop_fn)(ptr) };
            }
        }
    }
}

impl LinkedBytecodeView for NativeLinkedBytecode {
    fn bytes(&self) -> &[u8] {
        &self.code
    }

    fn instructions(&self) -> &[LinkedInstructionEntry] {
        &self.ordinals
    }

    fn metadata_types(&self) -> &[MetadataTypeSpec] {
        &self.metadata_types
    }

    fn metadata_counts(&self) -> &[u32] {
        &self.metadata_counts
    }

    fn metadata_bytes(&self) -> &[u8] {
        self.metadata.as_slice()
    }

    fn metadata_bytes_mut(&mut self) -> &mut [u8] {
        self.metadata.as_slice_mut()
    }

    fn metadata_table_mut(&mut self) -> MetadataTableViewMut<'_> {
        MetadataTableViewMut::new(
            self.metadata.as_slice_mut(),
            &self.metadata_types,
            &self.metadata_counts,
        )
    }

    fn metadata_typed_mut<T>(&mut self, opcode: usize, slot: u32) -> Option<&mut T> {
        let spec = self.metadata_types.get(opcode)?;
        debug_assert_metadata_type_matches::<T>(spec);
        let header = MetadataTableView::new(
            self.metadata.as_slice(),
            &self.metadata_types,
            &self.metadata_counts,
        )
        .header(opcode)?;
        if slot >= header.len {
            return None;
        }
        if spec.size == 0 {
            let ptr = NonNull::<T>::dangling().as_ptr();
            return Some(unsafe { &mut *ptr });
        }
        let bytes = self
            .metadata
            .as_slice_mut()
            .get_mut(metadata_slot_range(spec, header, slot)?)?;
        Some(unsafe { &mut *(bytes.as_mut_ptr().cast::<T>()) })
    }
}

fn non_null_from_slice<T>(slice: &mut [T]) -> NonNull<T> {
    if slice.is_empty() {
        NonNull::dangling()
    } else {
        NonNull::new(slice.as_mut_ptr()).expect("slice pointer is never null")
    }
}

fn required_backend_buffer<T>(value: Option<T>, name: &str) -> Result<T, LinkError> {
    value.ok_or_else(|| LinkError::Backend(format!("{name} buffer was not allocated")))
}

impl LinkBackendV2 for NativeHeapBackend {
    type Bytecode = NativeLinkedBytecode;

    fn allocate(&mut self, spec: &LinkSpec) -> Result<LinkAllocations, LinkError> {
        let code = self.allocate_code(spec.code_size)?;
        let ordinals = self.allocate_ordinals(spec.instruction_count)?;
        let metadata = self.allocate_metadata(spec.metadata_layout)?;
        Ok(LinkAllocations {
            code,
            ordinals,
            metadata,
        })
    }

    fn commit(
        self,
        _allocations: LinkAllocations,
        spec: LinkedArtifactSpec,
    ) -> Result<Self::Bytecode, LinkError> {
        let metadata = required_backend_buffer(self.metadata, "metadata")?;
        if metadata.layout != spec.metadata_layout {
            return Err(LinkError::Backend(
                "backend metadata layout does not match link plan".to_string(),
            ));
        }
        Ok(NativeLinkedBytecode {
            code: required_backend_buffer(self.code, "code")?,
            metadata,
            metadata_types: spec.metadata_types,
            metadata_counts: spec.metadata_counts,
            ordinals: required_backend_buffer(self.ordinals, "instruction index")?,
        })
    }
}
