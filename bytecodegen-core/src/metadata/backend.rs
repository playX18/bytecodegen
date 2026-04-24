use std::alloc::{self, Layout};
use std::ptr::NonNull;
use std::slice;

use crate::error::LinkError;

use super::planning::{MetadataTypeSpec, debug_assert_metadata_type_matches, metadata_slot_range};
use super::views::{LinkedBytecodeView, LinkedInstructionEntry, MetadataTableView, MetadataTableViewMut};

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
