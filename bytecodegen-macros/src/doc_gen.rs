use proc_macro2::TokenStream as TokenStream2;
use quote::ToTokens;
use quote::quote;
use syn::Type;

use crate::doc::DocBlock;
use crate::model::{GroupModel, OpcodeModel, SectionModel};

pub(crate) fn render_doc_attrs(lines: &[String]) -> TokenStream2 {
    quote!(#(#[doc = #lines])*)
}

pub(crate) fn combine_doc_attrs(authored: &DocBlock, mut generated: Vec<String>) -> TokenStream2 {
    let mut lines = authored.lines.clone();
    if !lines.is_empty() && !generated.is_empty() {
        lines.push(String::new());
    }
    lines.append(&mut generated);
    render_doc_attrs(&lines)
}

pub(crate) fn render_type_name(ty: &Type) -> String {
    let mut rendered = ty.to_token_stream().to_string();
    for (from, to) in [
        (" :: ", "::"),
        (" < ", "<"),
        ("< ", "<"),
        (" >", ">"),
        (" ,", ","),
        ("( ", "("),
        (" )", ")"),
        ("[ ", "["),
        (" ]", "]"),
    ] {
        rendered = rendered.replace(from, to);
    }
    rendered
}

pub(crate) fn format_hex_bytes(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut rendered = String::new();
    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 {
            rendered.push(' ');
        }
        let _ = write!(&mut rendered, "0x{byte:02X}");
    }
    rendered
}

#[derive(Clone, Copy)]
pub(crate) enum DocLayoutWidth {
    Narrow,
    Wide16,
    Wide32,
}

impl DocLayoutWidth {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Narrow => "Narrow",
            Self::Wide16 => "Wide16",
            Self::Wide32 => "Wide32",
        }
    }

    pub(crate) fn operand_bytes(self) -> usize {
        match self {
            Self::Narrow => 1,
            Self::Wide16 => 2,
            Self::Wide32 => 4,
        }
    }

    pub(crate) fn header_bytes(self, opcode_value: u32) -> Option<Vec<u8>> {
        match self {
            Self::Narrow if opcode_value <= crate::MAX_NARROW_OPCODE => {
                Some(vec![opcode_value as u8])
            }
            Self::Narrow => None,
            Self::Wide16 => {
                let mut bytes = vec![0xFE];
                bytes.extend_from_slice(&(opcode_value as u16).to_le_bytes());
                Some(bytes)
            }
            Self::Wide32 => {
                let mut bytes = vec![0xFF];
                bytes.extend_from_slice(&opcode_value.to_le_bytes());
                Some(bytes)
            }
        }
    }
}

pub(crate) struct EncodedOperandDoc {
    pub name: String,
    pub type_name: String,
    pub optional: bool,
}

pub(crate) struct LayoutSegmentDoc {
    pub byte_width: usize,
    pub lines: Vec<String>,
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn render_html_code(value: &str) -> String {
    format!("<code>{}</code>", escape_html(value))
}

fn render_byte_width_label(byte_width: usize) -> String {
    if byte_width == 1 {
        "1 byte".to_string()
    } else {
        format!("{byte_width} bytes")
    }
}

fn render_layout_table_lines(segments: &[LayoutSegmentDoc]) -> Vec<String> {
    let mut lines = vec![
        "<table>".to_string(),
        "<thead>".to_string(),
        "<tr>".to_string(),
    ];

    for segment in segments {
        lines.push(format!(
            "<th colspan=\"{}\">{}</th>",
            segment.byte_width,
            render_byte_width_label(segment.byte_width)
        ));
    }

    lines.extend([
        "</tr>".to_string(),
        "</thead>".to_string(),
        "<tbody>".to_string(),
        "<tr>".to_string(),
    ]);

    for segment in segments {
        lines.push(format!(
            "<td colspan=\"{}\">{}</td>",
            segment.byte_width,
            segment.lines.join("<br>")
        ));
    }

    lines.extend([
        "</tr>".to_string(),
        "</tbody>".to_string(),
        "</table>".to_string(),
    ]);

    lines
}

pub(crate) fn encoded_operand_docs(opcode: &OpcodeModel) -> Vec<EncodedOperandDoc> {
    let mut operands = opcode
        .body
        .args
        .iter()
        .map(|field| EncodedOperandDoc {
            name: field.name(),
            type_name: render_type_name(&field.ty),
            optional: field.optional,
        })
        .collect::<Vec<_>>();

    if !opcode.body.metadata.is_empty() {
        operands.push(EncodedOperandDoc {
            name: "metadata_id".to_string(),
            type_name: "u32".to_string(),
            optional: false,
        });
    }

    operands
}

pub(crate) fn opcode_summary_doc_lines(opcode: &OpcodeModel, opcode_value: u32) -> Vec<String> {
    vec![
        format!(
            "Schema instruction `{}` in section `{}`.",
            opcode.name.upper_camel, opcode.section.upper_camel
        ),
        String::new(),
        "## Encoding".to_string(),
        format!(
            "Opcode id: `{opcode_value}`. Opcode ids are assigned from the final schema opcode order."
        ),
        "A single width applies to the instruction header and every encoded operand.".to_string(),
        "Each layout below is shown left-to-right in emitted byte order.".to_string(),
    ]
}

pub(crate) fn opcode_width_doc_lines(
    opcode: &OpcodeModel,
    opcode_value: u32,
    width: DocLayoutWidth,
) -> Vec<String> {
    let mut docs = vec![String::new(), format!("### `{}`", width.label())];

    let Some(header_bytes) = width.header_bytes(opcode_value) else {
        docs.push(format!(
            "`Narrow` layout is unavailable because opcode id `{opcode_value}` exceeds `0xFD`."
        ));
        return docs;
    };

    let operands = encoded_operand_docs(opcode);
    let total_bytes = header_bytes.len() + operands.len().saturating_mul(width.operand_bytes());
    docs.push(format!(
        "Total encoded size at this width: `{total_bytes}` bytes."
    ));
    docs.push(format!(
        "Header bytes: `{}`.",
        format_hex_bytes(&header_bytes)
    ));

    let mut segments = vec![LayoutSegmentDoc {
        byte_width: header_bytes.len(),
        lines: vec![
            render_html_code("opcode"),
            render_html_code(&format_hex_bytes(&header_bytes)),
        ],
    }];

    for operand in operands {
        let mut lines = vec![
            render_html_code(&operand.name),
            render_html_code(&operand.type_name),
        ];
        if operand.optional {
            lines.push("optional operand".to_string());
        }
        segments.push(LayoutSegmentDoc {
            byte_width: width.operand_bytes(),
            lines,
        });
    }

    docs.extend(render_layout_table_lines(&segments));
    docs
}

pub(crate) fn opcode_notes_doc_lines() -> Vec<String> {
    vec![
        String::new(),
        "## Notes".to_string(),
        "Layout tables are structural only: operand slots follow the selected width and do not attempt to reflect custom `Operand::encoded_len(width)` overrides.".to_string(),
        "Operand payload bytes are delegated to each operand type's `Operand` implementation. Optional operands use `OptionalOperand`.".to_string(),
        "Linked metadata payload structs are stored outside the instruction byte stream; when present, the instruction encodes only `metadata_id` as the final operand slot.".to_string(),
        "Declared `tmps` and `checkpoints` are schema/runtime concepts only and are not encoded into the instruction byte stream.".to_string(),
    ]
}

pub(crate) fn opcode_id_doc_lines(opcode: &OpcodeModel, opcode_value: u32) -> Vec<String> {
    vec![format!(
        "Opcode id `{opcode_value}` for instruction `{}` in section `{}`.",
        opcode.name.upper_camel, opcode.section.upper_camel
    )]
}

#[allow(dead_code)]
pub(crate) fn opcode_descriptor_doc_lines(opcode: &OpcodeModel, opcode_value: u32) -> Vec<String> {
    vec![
        format!(
            "Descriptor for instruction `{}` with opcode id `{opcode_value}`.",
            opcode.name.upper_camel
        ),
        "See the instruction type docs for emitted byte-layout details.".to_string(),
    ]
}

pub(crate) fn opcode_struct_doc_lines(opcode: &OpcodeModel, opcode_value: u32) -> Vec<String> {
    let mut docs = opcode_summary_doc_lines(opcode, opcode_value);
    for width in [
        DocLayoutWidth::Narrow,
        DocLayoutWidth::Wide16,
        DocLayoutWidth::Wide32,
    ] {
        docs.extend(opcode_width_doc_lines(opcode, opcode_value, width));
    }
    docs.extend(opcode_notes_doc_lines());
    docs
}

pub(crate) fn metadata_struct_doc_lines(opcode: &OpcodeModel) -> Vec<String> {
    vec![
        format!("Linked metadata payload for `{}`.", opcode.name.upper_camel),
        "This payload is stored in the linked metadata table, not in the instruction byte stream."
            .to_string(),
        "Instructions that reference this payload encode only `metadata_id` in their byte layout."
            .to_string(),
    ]
}

pub(crate) fn tmp_enum_doc_lines(opcode: &OpcodeModel) -> Vec<String> {
    vec![format!(
        "Temporary values exposed to generated interpreters for `{}`.",
        opcode.name.upper_camel
    )]
}

pub(crate) fn checkpoint_enum_doc_lines(opcode: &OpcodeModel) -> Vec<String> {
    vec![format!(
        "Named metadata checkpoints declared for `{}`.",
        opcode.name.upper_camel
    )]
}

pub(crate) fn group_doc_lines(group: &GroupModel) -> Vec<String> {
    vec![
        format!(
            "Opcode group `{}` in section `{}`.",
            group.name.upper_camel, group.section.upper_camel
        ),
        format!(
            "Members: {}.",
            group
                .members
                .iter()
                .map(|member| format!("`{}`", member.upper_camel))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    ]
}

pub(crate) fn group_predicate_doc_lines(group: &GroupModel) -> Vec<String> {
    vec![format!(
        "Returns whether `opcode` belongs to group `{}`.",
        group.name.upper_camel
    )]
}

pub(crate) fn section_doc_lines(section: &SectionModel) -> Vec<String> {
    vec![
        format!("Schema section `{}`.", section.name.upper_camel),
        if section.preserve_order {
            "Opcode ids preserve this section's source order.".to_string()
        } else {
            "Opcode ids in this section follow the macro's default category ordering.".to_string()
        },
    ]
}

pub(crate) fn define_interpreter_doc_lines() -> Vec<String> {
    vec![
        "Generates a loop-based decoder/dispatcher function for the current schema.".to_string(),
        String::new(),
        "The macro input contains:".to_string(),
        "1. A free function signature.".to_string(),
        "2. An `ip` block with `load` and `store(next_ip)` hooks that translate between your instruction cursor and raw pointers.".to_string(),
        "3. An `invalid(opcode, width)` arm for unknown opcode values.".to_string(),
        "4. One `Opcode(binding) => expr` arm per schema opcode, in schema order.".to_string(),
        String::new(),
        "The generated function owns the interpreter loop. On each iteration it evaluates `load`, decodes one instruction, evaluates `store(next_ip)`, then runs the selected opcode handler inline as a statement. Handler bodies can update interpreter state directly and use normal control flow such as `return`, `break`, or `continue`.".to_string(),
        String::new(),
        "## Example".to_string(),
        String::new(),
        "```".to_string(),
        "use bytecodegen::{Emit, bytecode_list};".to_string(),
        String::new(),
        "bytecode_list! {".to_string(),
        "    section Bytecode {".to_string(),
        "        Noop {".to_string(),
        "            args {}".to_string(),
        "        }".to_string(),
        "        Return {".to_string(),
        "            args {".to_string(),
        "                value: i32,".to_string(),
        "            }".to_string(),
        "        }".to_string(),
        "    }".to_string(),
        "}".to_string(),
        String::new(),
        "generated::define_interpreter! {".to_string(),
        "    fn eval(ip: &mut *const u8) -> i32;".to_string(),
        "    ip {".to_string(),
        "        load = { *ip };".to_string(),
        "        store(next_ip) = { *ip = next_ip; };".to_string(),
        "    }".to_string(),
        "    invalid(opcode, width) => panic!(\"invalid opcode {opcode} at {width:?}\"),"
            .to_string(),
        "    Noop(_op) => {},".to_string(),
        "    Return(op) => return op.value,".to_string(),
        "}".to_string(),
        String::new(),
        "let (_, bytes) = Return::new(42).emit_smallest_to_vec().unwrap();".to_string(),
        "let mut ip = bytes.as_ptr();".to_string(),
        "assert_eq!(eval(&mut ip), 42);".to_string(),
        "```".to_string(),
    ]
}

pub(crate) fn define_tail_interpreter_doc_lines() -> Vec<String> {
    vec![
        "Generates a decoder/dispatcher function for explicit tail-call interpreters."
            .to_string(),
        String::new(),
        "The input shape matches `define_interpreter!`, but each opcode handler expression is inlined directly into the generated dispatch. That lets handlers use `become` to tail-call the dispatcher again after updating interpreter state.".to_string(),
        String::new(),
        "Arms still must appear once each and in schema order. The `store(next_ip)` hook runs before the selected handler expression is evaluated.".to_string(),
        String::new(),
        "## Example".to_string(),
        String::new(),
        "```ignore".to_string(),
        "#![feature(explicit_tail_calls)]".to_string(),
        String::new(),
        "use bytecodegen::bytecode_list;".to_string(),
        String::new(),
        "bytecode_list! {".to_string(),
        "    section Bytecode {".to_string(),
        "        Noop {".to_string(),
        "            args {}".to_string(),
        "        }".to_string(),
        "        Return {".to_string(),
        "            args {".to_string(),
        "                value: i32,".to_string(),
        "            }".to_string(),
        "        }".to_string(),
        "    }".to_string(),
        "}".to_string(),
        String::new(),
        "generated::define_tail_interpreter! {".to_string(),
        "    fn eval(ip: &mut *const u8) -> i32;".to_string(),
        "    ip {".to_string(),
        "        load = { *ip };".to_string(),
        "        store(next_ip) = { *ip = next_ip; };".to_string(),
        "    }".to_string(),
        "    invalid(opcode, width) => panic!(\"invalid opcode {opcode} at {width:?}\"),"
            .to_string(),
        "    Noop(_op) => become eval(ip),".to_string(),
        "    Return(op) => op.value,".to_string(),
        "}".to_string(),
        "```".to_string(),
    ]
}
