use std::fmt;

use crate::byte_io::{ByteCursor, ByteSink};
use crate::error::DecodeError;
use crate::traits::Operand;
use crate::width::OperandWidth;

/// Slot index of an instruction's record in its opcode's metadata array.
///
/// Instructions of opcodes with a `metadata` block carry this as their final
/// operand. It encodes exactly like `u32`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MetadataId(pub u32);

impl fmt::Display for MetadataId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Operand for MetadataId {
    fn fits_width(value: &Self, width: OperandWidth) -> bool {
        <u32 as Operand>::fits_width(&value.0, width)
    }

    fn encode<S: ByteSink + ?Sized>(value: &Self, width: OperandWidth, output: &mut S) {
        <u32 as Operand>::encode(&value.0, width, output)
    }

    fn decode(width: OperandWidth, input: &mut ByteCursor<'_>) -> Result<Self, DecodeError> {
        <u32 as Operand>::decode(width, input).map(Self)
    }

    fn can_decode_from_ptr() -> bool {
        true
    }

    #[inline(always)]
    unsafe fn decode_from_ptr(width: OperandWidth, input: *const u8) -> (Self, *const u8) {
        let (val, next) = unsafe { <u32 as Operand>::decode_from_ptr(width, input) };
        (Self(val), next)
    }
}
