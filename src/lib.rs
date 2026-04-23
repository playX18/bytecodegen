extern crate self as bytecodegen;

pub use bytecodegen_core as core;
pub use bytecodegen_core::{
    ByteCursor, ByteSink, DebugId, DebugMap, DebugTable, Decode, DecodeError,
    DefaultOperandFormatter, Disassemble, DisassembleError, DisassemblerOptions, DisassemblyLabel,
    Emit, EmitError, Encode, FormatOperand, InstructionFormatSpec, InstructionSpec, Label,
    LinkError, LinkedArtifactSpec, LinkedBytecodeView, LinkedDebugInfo, LinkedInstructionEntry,
    LinkedProgram, MAX_INSTRUCTION_HEADER_BYTES, MetadataHeaderEntry, MetadataTableLayout,
    MetadataTableView, MetadataTableViewMut, MetadataTypeSpec, MnemonicCase, NativeHeapBackend,
    NativeLinkedBytecode, Operand, OperandFormatSpec, OperandWidth, OptionalOperand, Placement,
    RelocValue, RelocationField, RelocationKind, SliceSink, VerificationError, VerificationResult,
    Verifier, decode_instruction_header, decode_instruction_header_from_ptr,
    encode_instruction_header, initialize_metadata_table, instruction_header_len,
    instruction_opcode_fits_width, metadata_slot_ptr, plan_metadata_table,
    relocation_target_offset,
};

pub use bytecodegen_macros::Operand;
/// Declares one or more bytecode sections and generates the instruction types and helpers for them.
///
/// Each `section` expands into concrete instruction structs, a shared `Instruction` enum, and a
/// `generated` module containing decode, disassemble, and linking helpers.
///
/// # Example
///
/// ```
/// use bytecodegen::{Emit, OperandWidth, bytecode_list};
///
/// bytecode_list! {
///     section Bytecode {
///         LoadConst {
///             args {
///                 dst: u8,
///                 value: i16,
///             }
///         }
///     }
/// }
///
/// let instruction = LoadConst::new(0, 42);
/// let (width, bytes) = instruction.emit_smallest_to_vec().unwrap();
///
/// assert_eq!(width, OperandWidth::Narrow);
///
/// match generated::Instruction::decode(&bytes).unwrap() {
///     generated::Instruction::LoadConst(decoded) => {
///         assert_eq!(decoded.dst, 0);
///         assert_eq!(decoded.value, 42);
///     }
/// }
/// ```
pub use bytecodegen_macros::bytecode_list;
