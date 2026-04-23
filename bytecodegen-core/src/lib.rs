//! Core runtime primitives for bytecode schemas, encoding, disassembly, and linking.
//!
//! This crate is usually consumed through the top-level `bytecodegen` crate, where the
//! `bytecode_list!` macro generates concrete instruction types on top of the traits defined
//! here.
//!
//! The public API is organized around three concerns:
//! - Instruction encoding, decoding, and disassembly via [`Encode`], [`Decode`],
//!   [`Emit`], and [`Disassemble`].
//! - Metadata planning and linked artifacts via [`LinkBackendV2`], [`LinkedBytecodeView`],
//!   and the metadata-table helpers.
//!
//! Most users interact with these types indirectly through generated code, while custom
//! backends, operand types, and tooling can use the core building blocks directly.

extern crate self as bytecodegen;

mod byte_io;
mod debug;
mod disassembly;
mod encoding;
mod error;
mod metadata;
mod traits;
mod verify;
mod width;

pub use byte_io::{
    ByteCursor, ByteSink, SliceSink, decode_instruction_header, decode_instruction_header_from_ptr,
    encode_instruction_header,
};
pub use error::{DecodeError, DisassembleError, EmitError, LinkError};
pub use traits::{
    Decode, DefaultOperandFormatter, Disassemble, Emit, Encode, FormatOperand, InstructionSpec,
    Operand, OptionalOperand,
};
pub use width::{
    Label, MAX_INSTRUCTION_HEADER_BYTES, OperandWidth, RelocValue, RelocationField, RelocationKind,
    instruction_header_len, instruction_opcode_fits_width, relocation_target_offset,
};

pub use debug::{DebugId, DebugMap, DebugTable, LinkedDebugInfo, LinkedProgram};
pub use disassembly::{
    DisassemblerOptions, DisassemblyLabel, InstructionFormatSpec, MnemonicCase, OperandFormatSpec,
    opcode_mnemonic, rendered_bytes_width, write_disassembly_prefix,
};
pub use metadata::{
    LinkAllocations, LinkBackendV2, LinkSpec, LinkedArtifactSpec, LinkedBytecodeView,
    LinkedInstructionEntry, MetadataHeaderEntry, MetadataTableLayout, MetadataTableView,
    MetadataTableViewMut, MetadataTypeSpec, NativeHeapBackend, NativeLinkedBytecode, Placement,
    initialize_metadata_table, metadata_slot_ptr, plan_metadata_table,
};
pub use verify::{VerificationError, VerificationResult, Verifier};
