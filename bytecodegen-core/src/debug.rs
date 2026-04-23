use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Range;

/// Opaque index into a DebugTable. Same value = same debug info entry.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct DebugId(u32);

impl DebugId {
    /// Returns the raw index value.
    pub fn as_u32(self) -> u32 {
        self.0
    }
}

/// Deduplicated storage of debug info entries. Generic over any `T: Eq + Hash + Clone`.
/// Inserting a value that already exists returns the existing `DebugId`.
pub struct DebugTable<T> {
    entries: Vec<T>,
    index: HashMap<T, DebugId>,
}

impl<T: Eq + Hash + Clone> DebugTable<T> {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// Insert a value. If it already exists, returns the existing `DebugId`.
    pub fn insert(&mut self, value: T) -> DebugId {
        if let Some(&id) = self.index.get(&value) {
            return id;
        }
        let id = DebugId(self.entries.len() as u32);
        self.entries.push(value);
        self.index.insert(self.entries.last().unwrap().clone(), id);
        id
    }

    /// Look up a value by its `DebugId`.
    pub fn get(&self, id: DebugId) -> Option<&T> {
        self.entries.get(id.0 as usize)
    }

    /// Returns the number of unique entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if there are no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (DebugId, &T)> {
        self.entries
            .iter()
            .enumerate()
            .map(|(i, value)| (DebugId(i as u32), value))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (DebugId, &mut T)> {
        self.entries
            .iter_mut()
            .enumerate()
            .map(|(i, value)| (DebugId(i as u32), value))
    }
}

impl<T: Eq + Hash + Clone> Default for DebugTable<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Maps byte-offset ranges to `DebugId`s. Sorted, non-overlapping.
pub struct DebugMap {
    pub ranges: Vec<(Range<usize>, DebugId)>,
}

impl DebugMap {
    /// Create a `DebugMap` from an iterator of (range, DebugId) pairs.
    /// Ranges are sorted automatically. Overlapping ranges panic in debug mode.
    pub fn from_ranges(ranges: impl IntoIterator<Item = (Range<usize>, DebugId)>) -> Self {
        let mut ranges: Vec<_> = ranges.into_iter().collect();
        // Sort by start offset
        ranges.sort_by_key(|(r, _)| r.start);
        // Validate non-overlapping in debug mode
        #[cfg(debug_assertions)]
        {
            for window in ranges.windows(2) {
                let (a, _) = &window[0];
                let (b, _) = &window[1];
                assert!(
                    a.end <= b.start,
                    "overlapping debug ranges: {:?} and {:?}",
                    a,
                    b
                );
            }
        }
        Self { ranges }
    }

    /// Look up the `DebugId` for a given byte offset.
    /// Returns `None` if the offset is not covered by any range.
    pub fn at_offset(&self, offset: usize) -> Option<DebugId> {
        let idx = self.ranges.partition_point(|(r, _)| r.start <= offset);
        if idx == 0 {
            return None;
        }
        let (range, id) = &self.ranges[idx - 1];
        if offset < range.end { Some(*id) } else { None }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Range<usize>, DebugId)> {
        self.ranges.iter().map(|(r, id)| (r, *id))
    }
}

/// Bundles a `DebugTable` and `DebugMap` for lookup by byte offset.
pub struct LinkedDebugInfo<T> {
    pub table: DebugTable<T>,
    pub map: DebugMap,
}

impl<T: Eq + Hash + Clone> LinkedDebugInfo<T> {
    pub fn new(table: DebugTable<T>, map: DebugMap) -> Self {
        Self { table, map }
    }

    /// Look up debug info for a given byte offset.
    pub fn at_offset(&self, offset: usize) -> Option<&T> {
        let id = self.map.at_offset(offset)?;
        self.table.get(id)
    }
}

/// Wraps a linked bytecode artifact together with its optional debug information.
///
/// When a section declares a `debug:` type, the generated `link` function returns
/// `LinkedProgram<DebugType>` instead of the raw `LinkedBytecode`.
pub struct LinkedProgram<Debug> {
    /// The linked bytecode artifact.
    pub bytecode: crate::metadata::NativeLinkedBytecode,
    /// Debug information collected during linking, if any.
    pub debug_info: Option<LinkedDebugInfo<Debug>>,
}

impl<Debug> LinkedProgram<Debug> {
    /// Creates a `LinkedProgram` with debug info.
    pub fn new(
        bytecode: crate::metadata::NativeLinkedBytecode,
        debug_info: LinkedDebugInfo<Debug>,
    ) -> Self {
        Self {
            bytecode,
            debug_info: Some(debug_info),
        }
    }

    /// Creates a `LinkedProgram` without debug info.
    pub fn without_debug(bytecode: crate::metadata::NativeLinkedBytecode) -> Self {
        Self {
            bytecode,
            debug_info: None,
        }
    }
}

impl<Debug: Eq + Hash + Clone> LinkedProgram<Debug> {
    /// Look up debug info for a given byte offset.
    pub fn debug_at_offset(&self, offset: usize) -> Option<&Debug> {
        self.debug_info.as_ref()?.at_offset(offset)
    }
}

impl<Debug> crate::metadata::LinkedBytecodeView for LinkedProgram<Debug> {
    fn bytes(&self) -> &[u8] {
        self.bytecode.bytes()
    }

    fn instructions(&self) -> &[crate::metadata::LinkedInstructionEntry] {
        self.bytecode.instructions()
    }

    fn metadata_types(&self) -> &[crate::metadata::MetadataTypeSpec] {
        self.bytecode.metadata_types()
    }

    fn metadata_counts(&self) -> &[u32] {
        self.bytecode.metadata_counts()
    }

    fn metadata_bytes(&self) -> &[u8] {
        self.bytecode.metadata_bytes()
    }

    fn metadata_bytes_mut(&mut self) -> &mut [u8] {
        self.bytecode.metadata_bytes_mut()
    }
}
