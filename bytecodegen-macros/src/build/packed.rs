use bytecodegen_core::{PackedFieldRef, PackedOperandSpec, PackedWordSpec, plan_packed_layout};
use syn::{Error, Result, Type};

use crate::model::{BodyModel, EncodingModel, NameModel};

pub(crate) fn packed_type_bits(ty: &Type) -> Result<u8> {
    let normalized = quote::quote!(#ty).to_string().replace(' ', "");
    match normalized.as_str() {
        "bool" | "u8" | "i8" => Ok(8),
        "u16" | "i16" => Ok(16),
        "u32" | "i32" | "usize" | "isize" => Ok(32),
        _ => Err(Error::new_spanned(
            ty,
            "packed encoding currently supports bool, u8/u16/u32, i8/i16/i32, usize, and isize",
        )),
    }
}

pub(crate) fn plan_body_packed_layout(
    name: &NameModel,
    body: &BodyModel,
    encoding: EncodingModel,
) -> Result<Option<bytecodegen_core::PackedLayoutPlan>> {
    match encoding {
        EncodingModel::Packed { word_bits } => {
            let operands = body
                .args
                .iter()
                .map(|field| {
                    Ok(PackedOperandSpec::required(
                        field.ident.to_string(),
                        packed_type_bits(&field.ty)?,
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            let explicit = body.layout.as_ref().map(lower_layout_model_to_core_specs);
            let packed_layout =
                plan_packed_layout(&name.upper_camel, word_bits, &operands, explicit.as_deref())
                    .map_err(|error| Error::new(name.ident.span(), error.to_string()))?;
            Ok(Some(packed_layout))
        }
        EncodingModel::Bytes => Ok(None),
    }
}

pub(crate) fn lower_layout_model_to_core_specs(layout: &crate::model::LayoutModel) -> Vec<PackedWordSpec> {
    layout
        .words
        .iter()
        .map(|word| {
            let fields = word
                .slices
                .iter()
                .map(|slice| match slice.name.as_str() {
                    "op" => PackedFieldRef::opcode(slice.width_bits),
                    "_" => PackedFieldRef::padding(slice.width_bits),
                    _ => PackedFieldRef::operand(&slice.name, slice.width_bits),
                })
                .collect::<Vec<_>>();
            PackedWordSpec::new(&fields)
        })
        .collect()
}
