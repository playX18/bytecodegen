use std::fmt;

use crate::width::OperandWidth;

/// Naming style used when rendering mnemonics during disassembly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MnemonicCase {
    /// Render mnemonics in `snake_case`.
    SnakeCase,
    /// Render mnemonics in `kebab-case`.
    KebabCase,
    /// Render mnemonics in `UpperCamel`.
    UpperCamel,
}

/// Label style emitted before each disassembled instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DisassemblyLabel {
    /// Label instructions by starting byte offset.
    ByteOffset,
    /// Label instructions by ordinal index.
    InstructionIndex,
}

/// Configuration for generated disassembly output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DisassemblerOptions {
    /// Whether to print a label column.
    pub include_labels: bool,
    /// Which value to use for labels when labels are enabled.
    pub label_kind: DisassemblyLabel,
    /// Minimum width used when formatting labels.
    pub label_width: usize,
    /// Offset added to the displayed label value.
    pub label_origin: usize,
    /// Whether to print the encoded bytes column.
    pub show_bytes: bool,
    /// Case to use for opcode mnemonics.
    pub mnemonic_case: MnemonicCase,
}

impl Default for DisassemblerOptions {
    fn default() -> Self {
        Self {
            include_labels: true,
            label_kind: DisassemblyLabel::ByteOffset,
            label_width: 3,
            label_origin: 0,
            show_bytes: true,
            mnemonic_case: MnemonicCase::SnakeCase,
        }
    }
}

/// Context passed to instruction-level formatting code during disassembly.
#[derive(Debug, Clone, Copy)]
pub struct InstructionFormatSpec<'a> {
    /// Zero-based instruction ordinal within the decoded stream.
    pub ordinal: usize,
    /// Starting byte offset of the instruction.
    pub offset: usize,
    /// Total encoded byte length of the instruction.
    pub size: usize,
    /// Width used for the header and operands.
    pub width: OperandWidth,
    /// Decoded opcode name in `snake_case`.
    pub opcode_name: &'a str,
    /// Exact bytes that were decoded for this instruction.
    pub raw_bytes: &'a [u8],
    /// Exact packed words that were decoded for this instruction, when applicable.
    pub raw_words: Option<&'a [u32]>,
    /// Width reserved for the rendered raw-encoding column.
    pub bytes_column_width: usize,
}

/// Context passed to operand-formatting code during disassembly.
#[derive(Debug, Clone, Copy)]
pub struct OperandFormatSpec<'a> {
    /// Instruction-level formatting context.
    pub instruction: InstructionFormatSpec<'a>,
    /// Zero-based operand index within the instruction's argument list.
    pub operand_index: usize,
}

fn snake_case_to_kebab_case(value: &str) -> String {
    value.replace('_', "-")
}

fn snake_case_to_upper_camel_case(value: &str) -> String {
    let mut output = String::new();
    for segment in value.split('_') {
        let mut chars = segment.chars();
        if let Some(first) = chars.next() {
            output.push(first.to_ascii_uppercase());
            output.push_str(chars.as_str());
        }
    }
    output
}

/// Returns the mnemonic spelling for `opcode_name` in the requested case.

pub fn opcode_mnemonic(opcode_name: &str, mnemonic_case: MnemonicCase) -> String {
    match mnemonic_case {
        MnemonicCase::SnakeCase => opcode_name.to_string(),
        MnemonicCase::KebabCase => snake_case_to_kebab_case(opcode_name),
        MnemonicCase::UpperCamel => snake_case_to_upper_camel_case(opcode_name),
    }
}

/// Writes the standard label, bytes, and mnemonic prefix used by generated disassemblers.
pub fn write_disassembly_prefix(
    out: &mut dyn fmt::Write,
    instruction: InstructionFormatSpec<'_>,
    options: &DisassemblerOptions,
) -> fmt::Result {
    if options.include_labels {
        let label = match options.label_kind {
            DisassemblyLabel::ByteOffset => instruction.offset + options.label_origin,
            DisassemblyLabel::InstructionIndex => instruction.ordinal + options.label_origin,
        };
        write!(out, "[{:0width$}] ", label, width = options.label_width)?;
    }

    if options.show_bytes {
        let mut rendered = String::new();
        if let Some(raw_words) = instruction.raw_words {
            for (index, word) in raw_words.iter().enumerate() {
                if index > 0 {
                    rendered.push(' ');
                }
                use fmt::Write as _;
                write!(&mut rendered, "0x{word:08x}")?;
            }
        } else {
            for (index, byte) in instruction.raw_bytes.iter().enumerate() {
                if index > 0 {
                    rendered.push(' ');
                }
                use fmt::Write as _;
                write!(&mut rendered, "0x{byte:02x}")?;
            }
        }

        write!(
            out,
            "{rendered:<width$} ",
            width = instruction.bytes_column_width
        )?;
    }

    let mnemonic = opcode_mnemonic(instruction.opcode_name, options.mnemonic_case);
    write!(out, "{mnemonic}")
}

/// Returns the display width of `raw_bytes` when rendered as `0xNN` groups separated by spaces.

pub fn rendered_bytes_width(raw_bytes: &[u8]) -> usize {
    if raw_bytes.is_empty() {
        0
    } else {
        (raw_bytes.len() * 4) + (raw_bytes.len() - 1)
    }
}

/// Returns the display width of `raw_words` when rendered as `0xNNNNNNNN` groups separated by spaces.
pub fn rendered_words_width(raw_words: &[u32]) -> usize {
    if raw_words.is_empty() {
        0
    } else {
        (raw_words.len() * 10) + (raw_words.len() - 1)
    }
}
