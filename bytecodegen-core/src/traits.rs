use std::fmt;

use crate::byte_io::{ByteCursor, ByteSink};
use crate::disassembly::{DisassemblerOptions, InstructionFormatSpec, OperandFormatSpec};
use crate::error::{DecodeError, EmitError};
use crate::width::OperandWidth;

/// Trait for operand encoding and decoding.
pub trait Operand: Sized + Copy {
    /// Returns whether `value` can be encoded at `width`.
    fn fits_width(value: &Self, width: OperandWidth) -> bool;
    /// Encodes `value` into `output` using `width`.
    fn encode<S: ByteSink + ?Sized>(value: &Self, width: OperandWidth, output: &mut S);
    /// Decodes a value of this type from `input` using `width`.
    fn decode(width: OperandWidth, input: &mut ByteCursor<'_>) -> Result<Self, DecodeError>;

    /// Returns encode size of `value` at `width`.
    fn encoded_len(width: OperandWidth) -> usize {
        width.bytes()
    }

    /// Returns whether this type supports the pointer-based decode fast path.
    ///
    /// Defaults to `false`. Types that want the pointer fast path should override
    /// this to return `true` and provide their own `decode_from_ptr`.  
    fn can_decode_from_ptr() -> bool {
        false
    }

    /// # Safety
    /// Caller must ensure `input` points to valid, aligned memory with enough
    /// bytes for the operand at the given width. Only call if `can_decode_from_ptr()`
    /// returns `true`.
    unsafe fn decode_from_ptr(width: OperandWidth, input: *const u8) -> (Self, *const u8) {
        let _ = (width, input);
        unreachable!("decode_from_ptr called on type that returns false for can_decode_from_ptr")
    }
}

/// Trait for optional operand encoding and decoding.
pub trait OptionalOperand: Operand {
    fn encode_optional<S: ByteSink + ?Sized>(
        value: Option<&Self>,
        width: OperandWidth,
        output: &mut S,
    );
    fn decode_optional(
        width: OperandWidth,
        input: &mut ByteCursor<'_>,
    ) -> Result<Option<Self>, DecodeError>;
    fn fits_optional(value: Option<&Self>, width: OperandWidth) -> bool;

    /// Returns whether this type supports the pointer-based optional decode fast path.
    ///
    /// Defaults to `false`. Types that want the pointer fast path should override
    /// this to return `true` and provide their own `decode_optional_from_ptr`.
    fn can_decode_from_ptr_optional() -> bool {
        false
    }

    /// # Safety
    /// Caller must ensure `input` points to valid, aligned memory with enough
    /// bytes for the operand at the given width. Only call if
    /// `can_decode_from_ptr_optional()` returns `true`.
    unsafe fn decode_optional_from_ptr(
        width: OperandWidth,
        input: *const u8,
    ) -> (Option<Self>, *const u8) {
        let _ = (width, input);
        unreachable!(
            "decode_optional_from_ptr called on type that returns false for can_decode_from_ptr_optional"
        )
    }
}

/// Custom operand formatter used by generated disassemblers.
pub trait FormatOperand<T, Ctx = ()> {
    /// Writes a formatted representation of `value` into `out`.
    fn format_operand(
        &self,
        value: &T,
        context: &Ctx,
        spec: OperandFormatSpec<'_>,
        out: &mut dyn fmt::Write,
    ) -> fmt::Result;
}

/// Formatter that delegates to [`fmt::Display`].
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultOperandFormatter;

impl<T, Ctx> FormatOperand<T, Ctx> for DefaultOperandFormatter
where
    T: fmt::Display,
{
    fn format_operand(
        &self,
        value: &T,
        _context: &Ctx,
        _spec: OperandFormatSpec<'_>,
        out: &mut dyn fmt::Write,
    ) -> fmt::Result {
        write!(out, "{value}")
    }
}

/// Common metadata hook implemented by generated instruction types.
pub trait InstructionSpec: Sized {
    /// Metadata payload associated with the instruction when linked.
    type Metadata: 'static;
}

/// Byte encoding support for complete instruction values.
pub trait Encode {
    /// Returns the byte length of this instruction at `width`.
    fn encoded_len(&self, width: OperandWidth) -> usize;

    /// Encodes this instruction body into `output` using `width`.
    fn encode_with_width<S: ByteSink + ?Sized>(&self, output: &mut S, width: OperandWidth);

    /// Encodes this instruction into a fresh [`Vec`].
    fn encode_to_vec(&self, width: OperandWidth) -> Vec<u8> {
        let mut output = Vec::with_capacity(self.encoded_len(width));
        self.encode_with_width(&mut output, width);
        output
    }
}

/// Byte decoding support for complete instruction values.
pub trait Decode: Sized {
    /// Decodes an instruction body after the header has already been consumed.
    fn decode_body(input: &mut ByteCursor<'_>, width: OperandWidth) -> Result<Self, DecodeError>;
}

/// Emission support for instructions that can choose an operand width.
pub trait Emit: Encode {
    /// Returns whether this instruction can be emitted at `width`.
    fn check_width(&self, width: OperandWidth) -> bool;

    /// Emits the full instruction, including its header, at `width`.
    fn emit_with_width<S: ByteSink + ?Sized>(
        &self,
        output: &mut S,
        width: OperandWidth,
    ) -> Result<(), EmitError>;

    /// Returns the smallest width that can encode this instruction.
    fn smallest_width(&self) -> Result<OperandWidth, EmitError>;

    /// Emits the instruction using [`Self::smallest_width`].
    fn emit_smallest<S: ByteSink + ?Sized>(
        &self,
        output: &mut S,
    ) -> Result<OperandWidth, EmitError> {
        let width = self.smallest_width()?;
        self.emit_with_width(output, width)?;
        Ok(width)
    }

    /// Emits the instruction using [`Self::smallest_width`] into a fresh [`Vec`].
    fn emit_smallest_to_vec(&self) -> Result<(OperandWidth, Vec<u8>), EmitError> {
        let width = self.smallest_width()?;
        let mut output = Vec::with_capacity(self.encoded_len(width));
        self.emit_with_width(&mut output, width)?;
        Ok((width, output))
    }
}

/// Disassembly support for instruction values.
pub trait Disassemble<Ctx, Formatter> {
    /// Writes a formatted instruction into `out`.
    fn disassemble_into(
        &self,
        out: &mut dyn fmt::Write,
        context: &Ctx,
        formatter: &Formatter,
        instruction: InstructionFormatSpec<'_>,
        options: &DisassemblerOptions,
    ) -> fmt::Result;

    /// Formats an instruction into a newly allocated [`String`].
    fn disassemble(
        &self,
        context: &Ctx,
        formatter: &Formatter,
        instruction: InstructionFormatSpec<'_>,
        options: &DisassemblerOptions,
    ) -> Result<String, fmt::Error>
    where
        Self: Sized,
    {
        let mut output = String::new();
        self.disassemble_into(&mut output, context, formatter, instruction, options)?;
        Ok(output)
    }
}
