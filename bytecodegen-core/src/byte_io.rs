use std::ops::Range;
use std::ptr;

use crate::error::DecodeError;
use crate::width::{OperandWidth, WIDE16_PREFIX, WIDE32_PREFIX, instruction_header_len};

/// Cursor over a byte slice used by decoders.
pub struct ByteCursor<'a> {
    input: &'a [u8],
    offset: usize,
}

impl<'a> ByteCursor<'a> {
    /// Creates a cursor positioned at the start of `input`.

    pub const fn new(input: &'a [u8]) -> Self {
        Self { input, offset: 0 }
    }

    /// Returns the number of unread bytes.

    pub const fn remaining(&self) -> usize {
        self.input.len().saturating_sub(self.offset)
    }

    /// Returns the current byte offset.

    pub const fn position(&self) -> usize {
        self.offset
    }

    /// Returns a slice of the original input without advancing the cursor.
    pub fn slice(&self, range: Range<usize>) -> Result<&'a [u8], DecodeError> {
        self.input.get(range).ok_or(DecodeError::UnexpectedEof)
    }

    /// Reads one byte in little-endian form.
    pub fn read_u8(&mut self) -> Result<u8, DecodeError> {
        if self.remaining() < 1 {
            return Err(DecodeError::UnexpectedEof);
        }
        let byte = self.input[self.offset];
        self.offset += 1;
        Ok(byte)
    }

    /// Reads an unsigned 16-bit integer in little-endian form.
    pub fn read_u16(&mut self) -> Result<u16, DecodeError> {
        let bytes = self.read_exact(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    /// Reads an unsigned 32-bit integer in little-endian form.
    pub fn read_u32(&mut self) -> Result<u32, DecodeError> {
        let bytes = self.read_exact(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_exact(&mut self, len: usize) -> Result<&'a [u8], DecodeError> {
        if self.remaining() < len {
            return Err(DecodeError::UnexpectedEof);
        }
        let start = self.offset;
        self.offset += len;
        Ok(&self.input[start..start + len])
    }

    pub(crate) fn read_unsigned(&mut self, width: OperandWidth) -> Result<u32, DecodeError> {
        Ok(match width {
            OperandWidth::Narrow => u32::from(self.read_u8()?),
            OperandWidth::Wide16 => u32::from(self.read_u16()?),
            OperandWidth::Wide32 => self.read_u32()?,
        })
    }

    pub(crate) fn read_signed(&mut self, width: OperandWidth) -> Result<i32, DecodeError> {
        Ok(match width {
            OperandWidth::Narrow => i32::from(self.read_u8()? as i8),
            OperandWidth::Wide16 => i32::from(self.read_u16()? as i16),
            OperandWidth::Wide32 => self.read_u32()? as i32,
        })
    }
}

/// Sink abstraction used by encoders and emitters.
pub trait ByteSink {
    /// Appends a single byte.
    fn push(&mut self, byte: u8);
    /// Appends a byte slice.
    fn extend_from_slice(&mut self, bytes: &[u8]);
}

impl ByteSink for Vec<u8> {
    fn push(&mut self, byte: u8) {
        Vec::push(self, byte);
    }

    fn extend_from_slice(&mut self, bytes: &[u8]) {
        Vec::extend_from_slice(self, bytes);
    }
}

/// [`ByteSink`] implementation backed by a caller-provided mutable slice.
pub struct SliceSink<'a> {
    output: &'a mut [u8],
    offset: usize,
}

impl<'a> SliceSink<'a> {
    /// Creates a sink that writes into `output` starting at offset `0`.

    pub fn new(output: &'a mut [u8]) -> Self {
        Self { output, offset: 0 }
    }

    /// Returns the next write position.

    pub const fn position(&self) -> usize {
        self.offset
    }
}

impl ByteSink for SliceSink<'_> {
    fn push(&mut self, byte: u8) {
        self.output[self.offset] = byte;
        self.offset += 1;
    }

    fn extend_from_slice(&mut self, bytes: &[u8]) {
        let end = self.offset + bytes.len();
        self.output[self.offset..end].copy_from_slice(bytes);
        self.offset = end;
    }
}

/// Encodes the header for a single instruction.
///
/// # Panics
///
/// Panics if `opcode` does not fit in `width`.
pub fn encode_instruction_header<S: ByteSink + ?Sized>(
    output: &mut S,
    opcode: u16,
    width: OperandWidth,
) {
    let _ = instruction_header_len(opcode, width);

    match width {
        OperandWidth::Narrow => output.push(opcode as u8),
        OperandWidth::Wide16 => {
            output.push(WIDE16_PREFIX);
            output.extend_from_slice(&opcode.to_le_bytes());
        }
        OperandWidth::Wide32 => {
            output.push(WIDE32_PREFIX);
            output.extend_from_slice(&u32::from(opcode).to_le_bytes());
        }
    }
}

/// Decodes an instruction header from `input`.
pub fn decode_instruction_header(
    input: &mut ByteCursor<'_>,
) -> Result<(OperandWidth, u32), DecodeError> {
    let first = input.read_u8()?;
    match first {
        WIDE16_PREFIX => Ok((OperandWidth::Wide16, u32::from(input.read_u16()?))),
        WIDE32_PREFIX => Ok((OperandWidth::Wide32, input.read_u32()?)),
        opcode => Ok((OperandWidth::Narrow, u32::from(opcode))),
    }
}

#[inline(always)]
pub(crate) unsafe fn read_ptr_u8(input: *const u8) -> u8 {
    unsafe { ptr::read(input) }
}

#[inline(always)]
pub(crate) unsafe fn read_ptr_u16(input: *const u8) -> u16 {
    unsafe { ptr::read_unaligned(input.cast::<u16>()) }
}

#[inline(always)]
pub(crate) unsafe fn read_ptr_u32(input: *const u8) -> u32 {
    unsafe { ptr::read_unaligned(input.cast::<u32>()) }
}

#[inline(always)]
/// Decodes an instruction header directly from a raw pointer.
///
/// The returned pointer is advanced past the header bytes.
///
/// # Safety
///
/// `input` must point to enough readable bytes for the decoded header form.
pub unsafe fn decode_instruction_header_from_ptr(
    input: *const u8,
) -> (OperandWidth, u32, *const u8) {
    let first = unsafe { read_ptr_u8(input) };
    match first {
        WIDE16_PREFIX => (
            OperandWidth::Wide16,
            u32::from(unsafe { read_ptr_u16(input.add(1)) }),
            unsafe { input.add(3) },
        ),
        WIDE32_PREFIX => (
            OperandWidth::Wide32,
            unsafe { read_ptr_u32(input.add(1)) },
            unsafe { input.add(5) },
        ),
        opcode => (OperandWidth::Narrow, u32::from(opcode), unsafe {
            input.add(1)
        }),
    }
}

#[cfg(debug_assertions)]
#[inline(always)]
pub(crate) fn assert_unsigned_ptr_decode_fits<T>(raw: u32, type_name: &'static str)
where
    T: TryFrom<u32>,
{
    assert!(
        T::try_from(raw).is_ok(),
        "decoded unsigned value {raw} does not fit {type_name}",
    );
}

#[cfg(debug_assertions)]
#[inline(always)]
pub(crate) fn assert_signed_ptr_decode_fits<T>(raw: i32, type_name: &'static str)
where
    T: TryFrom<i32>,
{
    assert!(
        T::try_from(raw).is_ok(),
        "decoded signed value {raw} does not fit {type_name}",
    );
}

#[cfg(debug_assertions)]
#[inline(always)]
pub(crate) fn assert_valid_bool_ptr_decode(raw: u32) {
    assert!(
        raw <= 1,
        "decoded boolean operand must be 0 or 1, found {raw}"
    );
}

#[cfg(debug_assertions)]
#[inline(always)]
pub(crate) fn assert_non_null_ptr_decode<T>(pointer: *mut T) {
    assert!(
        !pointer.is_null(),
        "decoded non-null pointer operand to null",
    );
}
