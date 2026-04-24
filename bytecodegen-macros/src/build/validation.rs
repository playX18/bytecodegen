use proc_macro2::Span;
use syn::{Error, Result};

use crate::model::EncodingModel;

pub(crate) const PACKED_OPCODE_CAPACITY: usize = (u8::MAX as usize) + 1;

pub(crate) fn validate_packed_encoding(packed: &crate::parse::PackedEncodingDecl) -> Result<()> {
    if packed.word_bits != 32 {
        return Err(Error::new(
            packed.word_bits_span,
            "packed encoding currently requires word_bits = 32",
        ));
    }
    Ok(())
}

pub(crate) fn validate_packed_opcode_count(
    encoding: EncodingModel,
    opcode_count: usize,
    span: Span,
) -> Result<()> {
    if matches!(encoding, EncodingModel::Packed { .. }) && opcode_count > PACKED_OPCODE_CAPACITY {
        return Err(Error::new(
            span,
            "packed encoding supports at most 256 opcodes because the `op` field is fixed at 8 bits",
        ));
    }

    Ok(())
}
