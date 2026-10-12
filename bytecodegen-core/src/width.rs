use std::sync::atomic::{AtomicU64, Ordering};

/// Operand and header width used for an instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperandWidth {
    /// One-byte operands and a one-byte instruction header.
    Narrow,
    /// Two-byte operands and a three-byte instruction header.
    Wide16,
    /// Four-byte operands and a five-byte instruction header.
    Wide32,
}

static NEXT_LABEL_ID: AtomicU64 = AtomicU64::new(0);

/// Opaque link-time symbol used to bind and resolve relocations inside one instruction stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Label(u64);

impl Label {
    /// Creates a fresh label that can be bound in a generated linker stream.
    pub fn new() -> Self {
        Self(NEXT_LABEL_ID.fetch_add(1, Ordering::Relaxed))
    }

    /// Returns the stable numeric identity carried by this label.
    pub const fn id(self) -> u64 {
        self.0
    }
}

impl Default for Label {
    fn default() -> Self {
        Self::new()
    }
}

/// Link-time operand value that is either already concrete or must be resolved from a label.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RelocValue<T> {
    /// Use the provided concrete operand value without relocation.
    Value(T),
    /// Resolve the operand from the bound position of `label`.
    Label(Label),
}

impl<T> From<T> for RelocValue<T> {
    fn from(value: T) -> Self {
        Self::Value(value)
    }
}

impl<T> RelocValue<T> {
    /// Creates a relocation input that resolves from `label`.
    pub const fn from_label(label: Label) -> Self {
        Self::Label(label)
    }
}

/// Relocation mode applied when converting a bound label into a concrete operand value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelocationKind {
    /// Store the bound label's absolute byte offset.
    Absolute,
    /// Store `label_offset - instruction_end_offset`.
    RelativeNext,
    /// Store `label_offset - instruction_offset`: the displacement from the
    /// start of the branching instruction, for VMs that add the offset to
    /// the branch's own address.
    RelativeStart,
}

impl RelocationKind {
    /// Computes the relocation value for a label using the current instruction layout.
    pub fn resolve(
        self,
        label_offset: usize,
        instruction_offset: usize,
        instruction_size: usize,
    ) -> Result<i128, crate::LinkError> {
        match self {
            Self::Absolute => Ok(label_offset as i128),
            Self::RelativeNext => {
                let base = instruction_offset.checked_add(instruction_size).ok_or(
                    crate::LinkError::RelocationComputationOverflow(
                        "instruction end offset overflowed during relocation",
                    ),
                )?;
                Ok((label_offset as i128) - (base as i128))
            }
            Self::RelativeStart => Ok((label_offset as i128) - (instruction_offset as i128)),
        }
    }
}

/// Converts a resolved relocation value into the operand type stored in an instruction.
pub trait RelocationField: Sized {
    /// Human-readable type name used in relocation diagnostics.
    const TYPE_NAME: &'static str;

    /// Converts a resolved relocation value into `Self`.
    fn from_relocation_value(value: i128) -> Result<Self, crate::LinkError>;

    /// Converts a decoded relocation operand back into its integer relocation value.
    fn to_relocation_value(&self) -> i128 {
        panic!(
            "RelocationField::to_relocation_value is required to disassemble relocatable `{}` operands",
            Self::TYPE_NAME
        )
    }
}

/// Resolves a decoded relocation operand to the destination byte offset it targets.

pub fn relocation_target_offset(
    kind: RelocationKind,
    encoded_value: i128,
    instruction_offset: usize,
    instruction_size: usize,
) -> Option<usize> {
    match kind {
        RelocationKind::Absolute => usize::try_from(encoded_value).ok(),
        RelocationKind::RelativeNext => {
            let base = instruction_offset.checked_add(instruction_size)? as i128;
            let target = base.checked_add(encoded_value)?;
            usize::try_from(target).ok()
        }
        RelocationKind::RelativeStart => {
            let target = (instruction_offset as i128).checked_add(encoded_value)?;
            usize::try_from(target).ok()
        }
    }
}

/// First byte of a wide16 instruction, the opcode follows as u16 LE.
pub const WIDE16_PREFIX: u8 = 0x00;
/// First byte of a wide32 instruction, the opcode follows as u32 LE.
pub const WIDE32_PREFIX: u8 = 0x01;
/// Lowest assignable opcode id. Ids 0 and 1 are the wide prefixes above,
/// never opcodes, so the first schema opcode is id 2.
pub const FIRST_OPCODE_ID: u16 = 2;

/// The maximum number of bytes needed for an instruction header.
pub const MAX_INSTRUCTION_HEADER_BYTES: usize = 5;

impl OperandWidth {
    /// Returns the number of bytes used by operands at this width.
    pub const fn bytes(self) -> usize {
        match self {
            Self::Narrow => 1,
            Self::Wide16 => 2,
            Self::Wide32 => 4,
        }
    }

    /// Returns widths from smallest to largest, which is the usual emission preference.
    pub const fn in_preference_order() -> [Self; 3] {
        [Self::Narrow, Self::Wide16, Self::Wide32]
    }

    /// Index of this width: 0 for narrow, 1 for wide16, 2 for wide32.
    pub const fn index(self) -> u8 {
        match self {
            Self::Narrow => 0,
            Self::Wide16 => 1,
            Self::Wide32 => 2,
        }
    }

    /// Width for an index. Any index above 2 gives `Wide32`.
    pub const fn from_index(index: u8) -> Self {
        match index {
            0 => Self::Narrow,
            1 => Self::Wide16,
            _ => Self::Wide32,
        }
    }
}

/// Returns whether `opcode` can be encoded in a header of `width`.
///
/// Narrow headers are one byte, so besides the reserved `WIDE16_PREFIX` and
/// `WIDE32_PREFIX` every id up to `0xFF` fits without widening the header.

pub const fn instruction_opcode_fits_width(opcode: u16, width: OperandWidth) -> bool {
    match width {
        OperandWidth::Narrow => opcode >= FIRST_OPCODE_ID && opcode <= u8::MAX as u16,
        OperandWidth::Wide16 | OperandWidth::Wide32 => true,
    }
}

/// Returns the encoded byte length of an instruction header.
///
/// # Panics
///
/// Panics if `opcode` does not fit in `width`.
pub fn instruction_header_len(opcode: u16, width: OperandWidth) -> usize {
    assert!(
        instruction_opcode_fits_width(opcode, width),
        "opcode {opcode} does not fit in {width:?} instruction header"
    );

    match width {
        OperandWidth::Narrow => 1,
        OperandWidth::Wide16 => 3,
        OperandWidth::Wide32 => 5,
    }
}
