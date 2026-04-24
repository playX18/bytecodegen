use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::Ident;

use crate::model::{FieldModel, OpcodeModel};

pub(crate) fn generate_packed_items(
    opcode: &OpcodeModel,
    crate_path: &TokenStream2,
) -> TokenStream2 {
    let Some(layout) = opcode.packed_layout.as_ref() else {
        return quote!();
    };

    let opcode_ident = &opcode.name.ident;
    let opcode_id_ident = &opcode.name.ident;
    let layout_ident = format_ident!("{}_PACKED_LAYOUT", opcode.name.screaming_snake_case);
    let placements_ident = format_ident!(
        "{}_PACKED_LAYOUT_PLACEMENTS",
        opcode.name.screaming_snake_case
    );
    let word_count_ident = format_ident!("{}_PACKED_WORD_COUNT", opcode.name.screaming_snake_case);
    let word_bits = layout.word_bits;
    let word_count = layout.word_count;
    let placement_count = layout.placements.len();

    let mut operand_layout_bits = std::collections::HashMap::<String, u8>::new();
    for placement in &layout.placements {
        if let bytecodegen_core::PackedFieldPlacement::Operand { name, bit_width, .. } = placement {
            *operand_layout_bits.entry(name.clone()).or_insert(0) += bit_width;
        }
    }

    let placement_items = layout.placements.iter().map(|placement| match placement {
        bytecodegen_core::PackedFieldPlacement::Opcode {
            word_index,
            bit_offset,
            bit_width,
        } => {
            quote! {
                #crate_path::core::PackedFieldPlacementRef::Opcode {
                    word_index: #word_index,
                    bit_offset: #bit_offset,
                    bit_width: #bit_width,
                }
            }
        }
        bytecodegen_core::PackedFieldPlacement::Operand {
            name,
            word_index,
            bit_offset,
            bit_width,
            operand_bit_offset,
        } => {
            quote! {
                #crate_path::core::PackedFieldPlacementRef::Operand {
                    name: #name,
                    word_index: #word_index,
                    bit_offset: #bit_offset,
                    bit_width: #bit_width,
                    operand_bit_offset: #operand_bit_offset,
                }
            }
        }
    });

    let raw_decls = opcode.body.args.iter().map(|field| {
        let raw_ident = packed_raw_ident(field);
        quote!(let mut #raw_ident = 0u32;)
    });

    let encode_raw_values = opcode.body.args.iter().map(|field| {
        let raw_ident = packed_raw_ident(field);
        let field_ident = &field.ident;
        let ty = &field.ty;
        let layout_width = operand_layout_bits.get(&field.ident.to_string()).copied();
        if field.optional {
            match layout_width {
                Some(width) => quote!(
                    let #raw_ident =
                        <#ty as #crate_path::core::PackedOptionalOperand>::encode_packed_optional_width(
                            self.#field_ident.as_ref(),
                            #width,
                        )?;
                ),
                None => quote!(
                    let #raw_ident =
                        <#ty as #crate_path::core::PackedOptionalOperand>::encode_packed_optional(
                            self.#field_ident.as_ref(),
                        )?;
                ),
            }
        } else {
            match layout_width {
                Some(width) => quote!(
                    let #raw_ident =
                        <#ty as #crate_path::core::PackedOperand>::encode_packed_width(
                            &self.#field_ident,
                            #width,
                        )?;
                ),
                None => quote!(
                    let #raw_ident =
                        <#ty as #crate_path::core::PackedOperand>::encode_packed(
                            &self.#field_ident)?;
                ),
            }
        }
    });

    let encode_placements = layout.placements.iter().map(|placement| match placement {
        bytecodegen_core::PackedFieldPlacement::Opcode {
            word_index,
            bit_offset,
            bit_width,
        } => {
            quote! {
                words[#word_index] = #crate_path::core::insert_packed_bits(
                    words[#word_index],
                    #crate_path::core::extract_packed_bits(opcode_raw, 0, #bit_width),
                    #bit_offset,
                    #bit_width,
                );
            }
        }
        bytecodegen_core::PackedFieldPlacement::Operand {
            name,
            word_index,
            bit_offset,
            bit_width,
            operand_bit_offset,
        } => {
            let raw_ident = packed_raw_ident(find_field(opcode, name));
            quote! {
                words[#word_index] = #crate_path::core::insert_packed_bits(
                    words[#word_index],
                    #crate_path::core::extract_packed_bits(#raw_ident, #operand_bit_offset, #bit_width),
                    #bit_offset,
                    #bit_width,
                );
            }
        }
    });

    let decode_placements = layout.placements.iter().map(|placement| match placement {
        bytecodegen_core::PackedFieldPlacement::Opcode {
            word_index,
            bit_offset,
            bit_width,
        } => {
            quote! {
                let opcode = #crate_path::core::extract_packed_bits(
                    words[#word_index],
                    #bit_offset,
                    #bit_width,
                );
                if opcode != opcode_raw {
                    return Err(#crate_path::core::DecodeError::InvalidOpcode(opcode));
                }
            }
        }
        bytecodegen_core::PackedFieldPlacement::Operand {
            name,
            word_index,
            bit_offset,
            bit_width,
            operand_bit_offset,
        } => {
            let raw_ident = packed_raw_ident(find_field(opcode, name));
            quote! {
                #raw_ident = #crate_path::core::insert_packed_bits(
                    #raw_ident,
                    #crate_path::core::extract_packed_bits(words[#word_index], #bit_offset, #bit_width),
                    #operand_bit_offset,
                    #bit_width,
                );
            }
        }
    });

    let decode_fields = opcode.body.args.iter().map(|field| {
        let raw_ident = packed_raw_ident(field);
        let field_ident = &field.ident;
        let ty = &field.ty;
        let layout_width = operand_layout_bits.get(&field.ident.to_string()).copied();
        if field.optional {
            match layout_width {
                Some(width) => quote!(
                    let #field_ident =
                        <#ty as #crate_path::core::PackedOptionalOperand>::decode_packed_optional_width(
                            #raw_ident,
                            #width,
                        )?;
                ),
                None => quote!(
                    let #field_ident =
                        <#ty as #crate_path::core::PackedOptionalOperand>::decode_packed_optional(
                            #raw_ident,
                        )?;
                ),
            }
        } else {
            match layout_width {
                Some(width) => quote!(
                    let #field_ident =
                        <#ty as #crate_path::core::PackedOperand>::decode_packed_width(
                            #raw_ident,
                            #width,
                        )?;
                ),
                None => quote!(
                    let #field_ident = <#ty as #crate_path::core::PackedOperand>::decode_packed(#raw_ident)?;
                ),
            }
        }
    });

    let struct_fields = opcode.body.args.iter().map(|field| {
        let field_ident = &field.ident;
        quote!(#field_ident,)
    });

    quote! {
        #[doc(hidden)]
        pub const #word_count_ident: usize = #word_count;

        #[doc(hidden)]
        pub const #placements_ident: [#crate_path::core::PackedFieldPlacementRef<'static>; #placement_count] = [
            #(#placement_items,)*
        ];

        pub static #layout_ident: std::sync::LazyLock<#crate_path::core::PackedLayoutRef<'static>> =
            std::sync::LazyLock::new(|| #crate_path::core::PackedLayoutRef {
                word_bits: #word_bits,
                word_count: #word_count_ident,
                placements: #placements_ident.to_vec(),
            });

        impl #opcode_ident {
            #[doc(hidden)]
            pub fn __packed_layout() -> #crate_path::core::PackedLayoutRef<'static> {
                (*#layout_ident).clone()
            }

            #[doc(hidden)]
            pub fn __encode_packed_words_impl(
                &self,
            ) -> Result<Vec<u32>, #crate_path::core::EmitError> {
                #(#encode_raw_values)*

                let opcode_raw = OpcodeId::#opcode_id_ident as u32;
                if opcode_raw > #crate_path::core::packed_mask(8) {
                    return Err(#crate_path::core::EmitError::PackedValueDoesNotFit {
                        type_name: "OpcodeId",
                        bit_width: 8,
                    });
                }

                let mut words = vec![0u32; #word_count_ident];
                #(#encode_placements)*
                Ok(words)
            }

            #[doc(hidden)]
            pub fn __decode_packed_words_impl(
                words: &[u32],
            ) -> Result<Self, #crate_path::core::DecodeError> {
                if words.len() != #word_count_ident {
                    return Err(#crate_path::core::DecodeError::InvalidValue(
                        "packed word input length must match opcode layout",
                    ));
                }

                let opcode_raw = OpcodeId::#opcode_id_ident as u32;
                #(#raw_decls)*
                #(#decode_placements)*
                #(#decode_fields)*

                Ok(Self {
                    #(#struct_fields)*
                })
            }

            pub fn encode_packed_to_words(&self) -> Result<Vec<u32>, #crate_path::core::EmitError> {
                <Self as #crate_path::core::PackedEncode>::encode_packed_to_words(self)
            }

            pub fn encode_packed_to_bytes(&self) -> Result<Vec<u8>, #crate_path::core::EmitError> {
                <Self as #crate_path::core::PackedEncode>::encode_packed_to_bytes(self)
            }

            pub fn decode_packed_words(
                words: &[u32],
            ) -> Result<Self, #crate_path::core::DecodeError> {
                <Self as #crate_path::core::PackedDecode>::decode_packed_words(words)
            }

            pub fn decode_packed_bytes(
                bytes: &[u8],
            ) -> Result<Self, #crate_path::core::DecodeError> {
                <Self as #crate_path::core::PackedDecode>::decode_packed_bytes(bytes)
            }
        }

        impl #crate_path::core::PackedEmit for #opcode_ident {
            fn packed_word_len(&self) -> usize {
                #word_count_ident
            }
        }

        impl #crate_path::core::PackedEncode for #opcode_ident {
            fn encode_packed_to_words(&self) -> Result<Vec<u32>, #crate_path::core::EmitError> {
                Self::__encode_packed_words_impl(self)
            }
        }

        impl #crate_path::core::PackedDecode for #opcode_ident {
            fn decode_packed_words(words: &[u32]) -> Result<Self, #crate_path::core::DecodeError> {
                Self::__decode_packed_words_impl(words)
            }
        }
    }
}

fn find_field<'a>(opcode: &'a OpcodeModel, name: &str) -> &'a FieldModel {
    opcode
        .body
        .args
        .iter()
        .find(|field| field.ident == name)
        .expect("packed layout references unknown operand")
}

fn packed_raw_ident(field: &FieldModel) -> Ident {
    format_ident!("__packed_raw_{}", field.ident)
}
