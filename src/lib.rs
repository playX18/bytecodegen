extern crate self as bytecodegen;

pub use bytecodegen_core as core;
pub use bytecodegen_core::{
    ByteCursor, ByteSink, DebugId, DebugMap, DebugTable, Decode, DecodeError,
    DefaultOperandFormatter, Disassemble, DisassembleError, DisassemblerOptions, DisassemblyLabel,
    Emit, EmitError, Encode, FIRST_OPCODE_ID, FormatOperand, InstructionFormatSpec,
    InstructionSpec, Label, LinkError, LinkedArtifactSpec, LinkedBytecodeView, LinkedDebugInfo,
    LinkedInstructionEntry, LinkedProgram, MAX_INSTRUCTION_HEADER_BYTES, MetadataHeaderEntry,
    MetadataId, MetadataTableLayout, MetadataTableView, MetadataTableViewMut, MetadataTypeSpec,
    MnemonicCase, NativeHeapBackend, NativeLinkedBytecode, Operand, OperandFormatSpec,
    OperandWidth, OptionalOperand, PackedDecode, PackedEmit, PackedEncode, PackedOperand,
    PackedOptionalOperand, Placement, RelocValue, RelocationField, RelocationKind, SliceSink,
    VerificationError, VerificationResult, Verifier, WIDE16_PREFIX, WIDE32_PREFIX,
    decode_instruction_header, decode_instruction_header_from_ptr, encode_instruction_header,
    extract_packed_bits, initialize_metadata_table, insert_packed_bits, instruction_header_len,
    instruction_opcode_fits_width, metadata_slot_ptr, packed_bytes_to_words, packed_mask,
    packed_words_to_bytes, plan_metadata_table, relocation_target_offset, rendered_words_width,
    sign_extend_packed,
};

pub use bytecodegen_macros::Operand;
/// Declares one or more bytecode sections and generates instruction types plus the helpers around
/// them.
///
/// `bytecode_list!` is the user-facing schema DSL for this crate. You describe the bytecode once,
/// and the macro generates typed instructions, encode/decode support, linking helpers for
/// relocations, metadata accessors, disassembly helpers, and section/opcode reflection data.
///
/// Each `section` expands into:
///
/// - one struct per opcode
/// - a section-local `Instruction` enum covering all opcodes in the schema
/// - a `LinkableInstruction` enum used during relocation and metadata linking
/// - a `generated` module with decode, disassemble, verify, and link helpers
/// - section/opcode/group descriptor tables used for reflection and tooling
///
/// # DSL shape
///
/// The top-level input is one or more `section` blocks:
///
/// ```text
/// bytecode_list! {
///     encoding = packed { word_bits = 32 };
///
///     section SectionName {
///         preserve_order = false;
///         debug: DebugInfo { ... }
///
///         OpcodeName {
///             args { ... }
///             metadata { ... }
///             tmps { ... }
///             checkpoints { ... }
///         }
///
///         group GroupName = [OpcodeA, OpcodeB] {
///             args { ... }
///         }
///     }
/// }
/// ```
///
/// Only `section` is required. Everything else is optional.
///
/// `encoding = packed { word_bits = 32 };` enables packed 32-bit word encoding instead of the
/// default variable-width byte stream. Packed schemas generate packed encode/decode helpers for
/// each opcode and for the section `Instruction` enum.
///
/// # Sections
///
/// A section groups related opcodes under one schema namespace.
///
/// - `section Name { ... }` declares a section.
/// - `preserve_order = true|false;` controls whether declared opcode order is kept as-is.
/// - `debug: DebugType { ... }` declares the per-instruction debug payload type used by linked
///   programs that carry debug info.
///
/// A schema may contain multiple sections. Opcode ids are assigned from the final schema-wide
/// opcode order after normalization.
///
/// # Opcodes
///
/// An opcode declaration looks like this:
///
/// ```text
/// LoadConst {
///     args {
///         dst: usize,
///         value: i32,
///     }
/// }
/// ```
///
/// Supported opcode body blocks:
///
/// - `args { ... }`: encoded operands stored in the instruction stream
/// - `metadata { ... }`: typed payload stored in the linked metadata table; the instruction stream
///   stores only `metadata_id`
/// - `tmps { ... }`: schema/runtime-only temporary fields, not encoded into bytecode
/// - `checkpoints { ... }`: named metadata checkpoints; requires a `metadata` block
/// - `layout { ... }`: explicit packed-word layout for packed schemas
///
/// Fields use `name: Type`. Optional fields use `name?: Type`.
///
/// Field attributes:
///
/// - `#[doc = "..."]` on opcodes and fields
/// - `#[relocatable]` or `#[relocatable(relative_next)]` for relocations
///   relative to the end of the instruction
/// - `#[relocatable(relative_start)]` for relocations relative to the start
///   of the instruction
/// - `#[relocatable(absolute)]` for absolute relocations
/// - `#[verify(stack_balance = N)]` on opcodes
/// - `#[verify(jump_target_valid)]` on opcodes
///
/// Relocatable fields are emitted as normal operands after linking, but are represented as
/// [`RelocValue`] in the linkable form before label resolution.
///
/// # Groups
///
/// A group declares several opcodes that share one operand layout:
///
/// ```text
/// group BinaryOp = [Add, Sub] {
///     args {
///         dst: usize,
///         lhs: usize,
///         rhs: usize,
///     }
/// }
/// ```
///
/// This expands into one concrete opcode per listed member and also generates group-level
/// reflection metadata.
///
/// # Width selection
///
/// Generated instruction types implement [`Encode`], [`Decode`], and [`Emit`].
/// Emission chooses the smallest instruction width that fits the opcode and every encoded operand:
/// [`OperandWidth::Narrow`], [`OperandWidth::Wide16`], or [`OperandWidth::Wide32`].
///
/// Widening applies to the whole instruction, not to individual fields.
///
/// # Packed layouts
///
/// Packed schemas are enabled with `encoding = packed { word_bits = 32 };`.
///
/// Without an explicit `layout { ... }` block, the macro synthesizes a layout automatically:
/// `op: 8` occupies the low bits of word 0, and operands are packed least-significant-bit first
/// across subsequent bit ranges and words.
///
/// An explicit layout is useful when the record format must be stable or externally documented:
///
/// ```text
/// Add {
///     args {
///         dst: u8,
///         lhs: u8,
///         rhs: u8,
///     }
///     layout {
///         word { op: 8, dst: 8, lhs: 8, rhs: 8 }
///     }
/// }
/// ```
///
/// Explicit packed layouts must include `op: 8` in the first word and must account for every
/// packed operand exactly once.
///
/// Packed optional operands currently use an all-zero sentinel for `None`, so the zero/default
/// representation is reserved for absence instead of an ordinary payload value.
///
/// # Generated items
///
/// The exact output depends on the schema, but the common user-facing pieces are:
///
/// - one struct per opcode, with a `new(...)` constructor
/// - a schema-local `generated::Instruction` enum
/// - `generated::link(...)` for relocations and metadata materialization
/// - `generated::verify(...)` for opcode verification rules
/// - disassembly and descriptor helpers in `generated`
/// - metadata structs such as `JumpTableMetadata` for opcodes with `metadata { ... }`
/// - for packed schemas, `encode_packed_to_words`, `encode_packed_to_bytes`,
///   `decode_packed_words`, and `decode_packed_bytes`
///
/// If the section declares `debug: ...`, linking can also produce a [`LinkedProgram`] carrying the
/// linked bytecode plus debug information.
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
///
///         group BinaryOp = [Add, Sub] {
///             args {
///                 dst: u8,
///                 lhs: u8,
///                 rhs: u8,
///             }
///         }
///     }
/// }
///
/// let instruction = LoadConst::new(0, 42);
/// let (width, bytes) = instruction.emit_smallest_to_vec().unwrap();
///
/// assert_eq!(width, OperandWidth::Narrow);
/// assert!(!bytes.is_empty());
/// ```
///
/// Packed encoding example:
///
/// ```
/// use bytecodegen::bytecode_list;
///
/// bytecode_list! {
///     encoding = packed { word_bits = 32 };
///
///     section Bytecode {
///         LoadConst {
///             args { dst: u8, value: i32 }
///         }
///
///         Add {
///             args { dst: u8, lhs: u8, rhs: u8 }
///             layout {
///                 word { op: 8, dst: 8, lhs: 8, rhs: 8 }
///             }
///         }
///     }
/// }
///
/// let words = LoadConst::new(0, 42).encode_packed_to_words().unwrap();
/// let decoded = LoadConst::decode_packed_words(&words).unwrap();
///
/// assert_eq!(decoded.dst, 0);
/// assert_eq!(decoded.value, 42);
/// ```
pub use bytecodegen_macros::bytecode_list;
