use proc_macro2::Span;
use syn::{Error, Result};

use crate::model::EncodingModel;

pub(crate) const PACKED_OPCODE_CAPACITY: usize = (u8::MAX as usize) + 1;
pub(crate) const BYTE_OPCODE_CAPACITY: usize = 512;

pub(crate) fn validate_packed_encoding(packed: &crate::parse::PackedEncodingDecl) -> Result<()> {
    if packed.word_bits != 32 {
        return Err(Error::new(
            packed.word_bits_span,
            "packed encoding currently requires word_bits = 32",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_encoding_rejects_more_than_512_opcodes() {
        let error = validate_opcode_count(
            EncodingModel::Bytes,
            BYTE_OPCODE_CAPACITY + 1,
            Span::call_site(),
        )
        .expect_err("byte encoding should reject too many opcodes");

        assert!(
            error
                .to_string()
                .contains("byte encoding supports at most 512 opcodes")
        );
    }
}

pub(crate) fn validate_opcode_count(
    encoding: EncodingModel,
    opcode_count: usize,
    span: Span,
) -> Result<()> {
    match encoding {
        EncodingModel::Bytes if opcode_count > BYTE_OPCODE_CAPACITY => {
            return Err(Error::new(
                span,
                "byte encoding supports at most 512 opcodes",
            ));
        }
        EncodingModel::Packed { .. } if opcode_count > PACKED_OPCODE_CAPACITY => {
            return Err(Error::new(
                span,
                "packed encoding supports at most 256 opcodes because the `op` field is fixed at 8 bits",
            ));
        }
        _ => {}
    }

    Ok(())
}
