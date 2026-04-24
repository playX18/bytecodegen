use std::fmt;

use crate::{Label, OperandWidth};

/// Errors produced while decoding bytecode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// Input ended before the requested value was fully available.
    UnexpectedEof,
    /// A width tag was syntactically present but invalid for the expected format.
    InvalidWidth(u8),
    /// An opcode id was not recognized by the generated schema.
    InvalidOpcode(u32),
    /// An instruction ordinal did not exist in the linked artifact.
    InvalidInstructionOrdinal(usize),
    /// A decoded primitive value violated an operand-type invariant.
    InvalidValue(&'static str),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof => write!(f, "unexpected end of input"),
            Self::InvalidWidth(width) => write!(f, "invalid width tag {width}"),
            Self::InvalidOpcode(opcode) => write!(f, "invalid opcode id {opcode}"),
            Self::InvalidInstructionOrdinal(ordinal) => {
                write!(f, "invalid instruction ordinal {ordinal}")
            }
            Self::InvalidValue(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Errors produced while disassembling instruction streams.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisassembleError {
    /// Bytecode could not be decoded.
    Decode(DecodeError),
    /// Rendering into the destination formatter failed.
    Format,
}

impl fmt::Display for DisassembleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(error) => write!(f, "{error}"),
            Self::Format => write!(f, "failed to format disassembly output"),
        }
    }
}

impl std::error::Error for DisassembleError {}

impl From<DecodeError> for DisassembleError {
    fn from(value: DecodeError) -> Self {
        Self::Decode(value)
    }
}

impl From<fmt::Error> for DisassembleError {
    fn from(_: fmt::Error) -> Self {
        Self::Format
    }
}

/// Errors produced while emitting encoded instructions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmitError {
    /// At least one operand or the opcode header did not fit in the requested width.
    WidthDoesNotFit {
        /// Opcode name used in the error message.
        opcode: &'static str,
        /// Width that failed validation.
        width: OperandWidth,
    },
    /// A packed primitive value did not fit the declared bit width.
    PackedValueDoesNotFit {
        /// Rust type name used in the error message.
        type_name: &'static str,
        /// Declared bit width.
        bit_width: u8,
    },
}

impl fmt::Display for EmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WidthDoesNotFit { opcode, width } => {
                write!(f, "{opcode} does not fit in {width:?}")
            }
            Self::PackedValueDoesNotFit {
                type_name,
                bit_width,
            } => {
                write!(f, "{type_name} does not fit in {bit_width} bits")
            }
        }
    }
}

impl std::error::Error for EmitError {}

/// Errors produced while linking encoded instructions into an artifact.
#[derive(Debug)]
pub enum LinkError {
    /// Instruction emission failed.
    Emit(EmitError),
    /// Metadata or allocation layout arithmetic overflowed.
    LayoutOverflow(&'static str),
    /// A relocation referenced a label that was never bound.
    UndefinedLabel(Label),
    /// A label was bound more than once in the same instruction stream.
    DuplicateLabel(Label),
    /// A resolved relocation could not be represented by the target operand type.
    RelocationValueOutOfRange {
        /// Operand type that rejected the resolved value.
        type_name: &'static str,
        /// Resolved relocation value before conversion.
        value: i128,
    },
    /// Relocation address arithmetic overflowed before conversion.
    RelocationComputationOverflow(&'static str),
    /// Backend-specific allocation or finalization failure.
    Backend(String),
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Emit(error) => write!(f, "{error}"),
            Self::LayoutOverflow(message) => write!(f, "{message}"),
            Self::UndefinedLabel(label) => write!(f, "undefined label {}", label.id()),
            Self::DuplicateLabel(label) => write!(f, "duplicate label {}", label.id()),
            Self::RelocationValueOutOfRange { type_name, value } => {
                write!(f, "relocation value {value} does not fit {type_name}")
            }
            Self::RelocationComputationOverflow(message) => write!(f, "{message}"),
            Self::Backend(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for LinkError {}

impl From<EmitError> for LinkError {
    fn from(value: EmitError) -> Self {
        Self::Emit(value)
    }
}
