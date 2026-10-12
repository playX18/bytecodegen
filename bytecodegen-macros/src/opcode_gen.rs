use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};

use crate::doc_gen::{
    checkpoint_enum_doc_lines, combine_doc_attrs, metadata_struct_doc_lines,
    opcode_descriptor_doc_lines, opcode_struct_doc_lines, render_doc_attrs, tmp_enum_doc_lines,
};
use crate::model::{CheckpointModel, FieldModel, OpcodeModel};
use crate::packed_gen::generate_packed_items;

pub(crate) fn generate_opcode_items(
    opcode: &OpcodeModel,
    opcode_value: u32,
    crate_path: &TokenStream2,
    has_debug: bool,
    metadata_kind: Option<usize>,
) -> TokenStream2 {
    let opcode_ident = &opcode.name.ident;
    let section_ident = &opcode.section.ident;
    let tmp_enum_ident = format_ident!("{}Tmp", opcode.name.ident);
    let checkpoint_enum_ident = format_ident!("{}Checkpoint", opcode.name.ident);
    let metadata_struct_ident = format_ident!("{}Metadata", opcode.name.ident);
    let metadata_id_ident = format_ident!("metadata_id");
    let has_metadata = !opcode.body.metadata.is_empty();
    let args_len = opcode.body.args.len() + usize::from(has_metadata);
    let metadata_id_docs = vec!["Linked metadata table slot for this instruction.".to_string()];
    // `opcode_value` is the schema position; the wire id adds the two
    // reserved prefix ids.
    let wire_value = opcode_value + crate::FIRST_OPCODE_ID;
    let table_index = opcode_value as usize;
    let descriptor_docs = render_doc_attrs(&opcode_descriptor_doc_lines(opcode, wire_value));
    let opcode_name = &opcode.name.snake_case;
    let opcode_mnemonic = &opcode.name.kebab_case;

    let opcode_docs = combine_doc_attrs(&opcode.docs, opcode_struct_doc_lines(opcode, wire_value));

    let metadata_struct = if opcode.body.metadata.is_empty() {
        quote!()
    } else {
        let metadata_fields = opcode.body.metadata.iter().map(|field| {
            let docs = render_doc_attrs(&field.docs.lines);
            let ident = &field.ident;
            let ty = &field.ty;
            quote!(#docs pub #ident: #ty,)
        });
        let docs = render_doc_attrs(&metadata_struct_doc_lines(opcode));

        quote! {
            #docs
            // No `Copy`: payload types are arbitrary, and `derive(Copy)`
            // rejects fields that are not `Copy` instead of bounding away.
            #[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
            pub struct #metadata_struct_ident {
                #(#metadata_fields)*
            }
        }
    };

    let tmp_enum = if opcode.body.tmps.is_empty() {
        quote!()
    } else {
        let variants = opcode.body.tmps.iter().map(|field| {
            let docs = render_doc_attrs(&field.docs.lines);
            let variant = field.variant_ident();
            quote!(#docs #variant,)
        });
        let values = opcode.body.tmps.iter().map(FieldModel::variant_ident);
        let docs = render_doc_attrs(&tmp_enum_doc_lines(opcode));
        quote! {
            #docs
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
            pub enum #tmp_enum_ident {
                #(#variants)*
            }

            impl #opcode_ident {
                pub const TMPS: &'static [#tmp_enum_ident] = &[
                    #(#tmp_enum_ident::#values,)*
                ];
            }
        }
    };

    let checkpoint_enum = if opcode.body.checkpoints.is_empty() {
        quote!()
    } else {
        let variants = opcode.body.checkpoints.iter().map(|checkpoint| {
            let docs = render_doc_attrs(&checkpoint.docs.lines);
            let variant = checkpoint.variant_ident();
            quote!(#docs #variant,)
        });
        let values = opcode
            .body
            .checkpoints
            .iter()
            .map(CheckpointModel::variant_ident);
        let docs = render_doc_attrs(&checkpoint_enum_doc_lines(opcode));
        quote! {
            #docs
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
            pub enum #checkpoint_enum_ident {
                #(#variants)*
            }

            impl #opcode_ident {
                pub const CHECKPOINTS: &'static [#checkpoint_enum_ident] = &[
                    #(#checkpoint_enum_ident::#values,)*
                ];
            }
        }
    };

    let mut struct_fields = opcode
        .body
        .args
        .iter()
        .map(|field| {
            let docs = render_doc_attrs(&field.docs.lines);
            let ident = &field.ident;
            let ty = &field.ty;
            if field.optional {
                quote!(#docs pub #ident: Option<#ty>,)
            } else {
                quote!(#docs pub #ident: #ty,)
            }
        })
        .collect::<Vec<_>>();
    if has_metadata {
        let docs = render_doc_attrs(&metadata_id_docs);
        struct_fields.push(quote!(#docs pub #metadata_id_ident: #crate_path::core::MetadataId,));
    }

    let new_params = opcode.body.args.iter().map(|field| {
        let ident = &field.ident;
        let ty = &field.ty;
        if field.optional {
            quote!(#ident: Option<#ty>)
        } else {
            quote!(#ident: #ty)
        }
    });

    let mut new_initializers = opcode
        .body
        .args
        .iter()
        .map(|field| {
            let ident = &field.ident;
            quote!(#ident,)
        })
        .collect::<Vec<_>>();
    if has_metadata {
        new_initializers.push(quote!(#metadata_id_ident: #crate_path::core::MetadataId(0),));
    }

    let metadata_helpers = if opcode.body.metadata.is_empty() {
        quote!()
    } else {
        let with_metadata_ctor = if has_debug {
            quote!(LinkableInstruction::#opcode_ident { opcode: self, metadata, debug_id: None })
        } else {
            quote!(LinkableInstruction::#opcode_ident { opcode: self, metadata })
        };
        quote! {
            pub fn with_metadata(self, metadata: #metadata_struct_ident) -> LinkableInstruction {
                #with_metadata_ctor
            }

            pub fn metadata<'a>(
                &'a self,
                table: &'a #crate_path::core::MetadataTableView<'_>,
            ) -> Option<&'a #metadata_struct_ident> {
                table.get_typed::<#metadata_struct_ident>(
                    #table_index,
                    self.#metadata_id_ident.0,
                )
            }

            pub fn metadata_mut<'a>(
                &'a self,
                table: &'a mut #crate_path::core::MetadataTableViewMut<'_>,
            ) -> Option<&'a mut #metadata_struct_ident> {
                table.get_typed_mut::<#metadata_struct_ident>(
                    #table_index,
                    self.#metadata_id_ident.0,
                )
            }
        }
    };

    let linkable_from = if opcode.body.metadata.is_empty() && !opcode.has_relocations() {
        if has_debug {
            quote! {
                impl From<#opcode_ident> for LinkableInstruction {
                    fn from(opcode: #opcode_ident) -> Self {
                        Self::#opcode_ident { opcode, debug_id: None }
                    }
                }
            }
        } else {
            quote! {
                impl From<#opcode_ident> for LinkableInstruction {
                    fn from(opcode: #opcode_ident) -> Self {
                        Self::#opcode_ident(opcode)
                    }
                }
            }
        }
    } else if !opcode.has_relocations() && has_debug {
        // Metadata-only variant with debug: the From impl is not generated because
        // the user must use with_metadata() which already constructs the struct variant.
        quote!()
    } else {
        quote!()
    };

    let metadata_type = if opcode.body.metadata.is_empty() {
        quote!(())
    } else {
        quote!(#metadata_struct_ident)
    };

    let mut encode_len_terms = opcode
        .body
        .args
        .iter()
        .map(|field| {
            let ty = &field.ty;
            quote!(<#ty as #crate_path::core::Operand>::encoded_len(width))
        })
        .collect::<Vec<_>>();
    if has_metadata {
        encode_len_terms.push(
            quote!(<#crate_path::core::MetadataId as #crate_path::core::Operand>::encoded_len(width)),
        );
    }

    let mut encode_args = opcode
        .body
        .args
        .iter()
        .map(|field| {
            let ident = &field.ident;
            let ty = &field.ty;
            if field.optional {
                quote!(<#ty as #crate_path::core::OptionalOperand>::encode_optional(self.#ident.as_ref(), width, output);)
            } else {
                quote!(<#ty as #crate_path::core::Operand>::encode(&self.#ident, width, output);)
            }
        })
        .collect::<Vec<_>>();
    if has_metadata {
        encode_args.push(quote!(
            <#crate_path::core::MetadataId as #crate_path::core::Operand>::encode(&self.#metadata_id_ident, width, output);
        ));
    }

    let mut check_args = opcode
        .body
        .args
        .iter()
        .map(|field| {
            let ident = &field.ident;
            let ty = &field.ty;
            if field.optional {
                quote!(<#ty as #crate_path::core::OptionalOperand>::fits_optional(self.#ident.as_ref(), width))
            } else {
                quote!(<#ty as #crate_path::core::Operand>::fits_width(&self.#ident, width))
            }
        })
        .collect::<Vec<_>>();
    if has_metadata {
        check_args.push(quote!(
            <#crate_path::core::MetadataId as #crate_path::core::Operand>::fits_width(&self.#metadata_id_ident, width)
        ));
    }

    let mut decode_args = opcode
        .body
        .args
        .iter()
        .map(|field| {
            let ident = &field.ident;
            let ty = &field.ty;
            if field.optional {
                quote!(let #ident = <#ty as #crate_path::core::OptionalOperand>::decode_optional(width, input)?;)
            } else {
                quote!(let #ident = <#ty as #crate_path::core::Operand>::decode(width, input)?;)
            }
        })
        .collect::<Vec<_>>();
    if has_metadata {
        decode_args.push(quote!(
            let #metadata_id_ident = <#crate_path::core::MetadataId as #crate_path::core::Operand>::decode(width, input)?;
        ));
    }

    let mut decode_fields = opcode
        .body
        .args
        .iter()
        .map(|field| {
            let ident = &field.ident;
            quote!(#ident,)
        })
        .collect::<Vec<_>>();
    if has_metadata {
        decode_fields.push(quote!(#metadata_id_ident,));
    }

    let emit_error_name = &opcode.name.upper_camel;
    let opcode_enum_ident = &opcode.name.ident;
    let has_relocations = opcode.has_relocations();
    let is_packed = opcode.packed_layout.is_some();
    let packed_items = generate_packed_items(opcode, crate_path);
    let reloc_variant_ident = format_ident!("__{}Linkable", opcode.name.ident);
    let reloc_input_traits = opcode.body.args.iter().filter_map(|field| {
        let ty = &field.ty;
        field.relocation.map(|_| {
            let trait_ident =
                format_ident!("__{}{}IntoReloc", opcode.name.ident, field.variant_ident());
            quote! {
                #[doc(hidden)]
                pub trait #trait_ident {
                    fn __into_reloc(self) -> #crate_path::core::RelocValue<#ty>;
                }

                impl #trait_ident for #ty {
                    fn __into_reloc(self) -> #crate_path::core::RelocValue<#ty> {
                        #crate_path::core::RelocValue::Value(self)
                    }
                }

                impl #trait_ident for #crate_path::core::Label {
                    fn __into_reloc(self) -> #crate_path::core::RelocValue<#ty> {
                        #crate_path::core::RelocValue::from_label(self)
                    }
                }

                impl #trait_ident for #crate_path::core::RelocValue<#ty> {
                    fn __into_reloc(self) -> #crate_path::core::RelocValue<#ty> {
                        self
                    }
                }
            }
        })
    });
    let mut disassemble_bounds = opcode
        .body
        .args
        .iter()
        .map(|field| {
            let ty = &field.ty;
            quote!(Formatter: #crate_path::core::FormatOperand<#ty, Ctx>,)
        })
        .collect::<Vec<_>>();
    if has_metadata {
        disassemble_bounds.push(
            quote!(Formatter: #crate_path::core::FormatOperand<#crate_path::core::MetadataId, Ctx>,),
        );
    }
    let mut disassemble_args = opcode
        .body
        .args
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let ident = &field.ident;
            let ty = &field.ty;
            let separator = if index == 0 {
                quote!(write!(output, " ")?;)
            } else {
                quote!(write!(output, ", ")?;)
            };

            if field.optional {
                quote! {
                    #separator
                    if let Some(value) = self.#ident.as_ref() {
                        let operand = #crate_path::core::OperandFormatSpec {
                            instruction,
                            operand_index: #index,
                        };
                        <Formatter as #crate_path::core::FormatOperand<#ty, Ctx>>::format_operand(
                            formatter,
                            value,
                            context,
                            operand,
                            output,
                        )?;
                    } else {
                        write!(output, "_")?;
                    }
                }
            } else {
                quote! {
                    #separator
                    let operand = #crate_path::core::OperandFormatSpec {
                        instruction,
                        operand_index: #index,
                    };
                    <Formatter as #crate_path::core::FormatOperand<#ty, Ctx>>::format_operand(
                        formatter,
                        &self.#ident,
                        context,
                        operand,
                        output,
                    )?;
                }
            }
        })
        .collect::<Vec<_>>();
    if has_metadata {
        let operand_index = syn::Index::from(args_len - 1);
        let separator = if args_len == 1 {
            quote!(write!(output, " ")?;)
        } else {
            quote!(write!(output, ", ")?;)
        };
        disassemble_args.push(quote! {
            #separator
            let operand = #crate_path::core::OperandFormatSpec {
                instruction,
                operand_index: #operand_index,
            };
            <Formatter as #crate_path::core::FormatOperand<#crate_path::core::MetadataId, Ctx>>::format_operand(
                formatter,
                &self.#metadata_id_ident,
                context,
                operand,
                output,
            )?;
        });
    }
    let reloc_fields = opcode
        .body
        .args
        .iter()
        .filter_map(|field| field.relocation.map(|relocation| (field, relocation)))
        .collect::<Vec<_>>();
    let disassemble_reloc_comments = if reloc_fields.is_empty() {
        quote!()
    } else {
        let single_reloc = reloc_fields.len() == 1;
        let comments = reloc_fields.iter().enumerate().map(|(index, (field, relocation))| {
            let ident = &field.ident;
            let ty = &field.ty;
            let field_name = field.name();
            let relocation_kind = relocation.to_core_tokens(crate_path);
            let separator = if index == 0 {
                quote!(write!(output, " ; ")?;)
            } else {
                quote!(write!(output, ", ")?;)
            };
            if single_reloc {
                quote! {
                    #separator
                    if let Some(__target_offset) = #crate_path::core::relocation_target_offset(
                        #relocation_kind,
                        <#ty as #crate_path::core::RelocationField>::to_relocation_value(&self.#ident),
                        instruction.offset,
                        instruction.size,
                    ) {
                        write!(output, "-> {}", __target_offset)?;
                    } else {
                        write!(output, "-> <invalid>")?;
                    }
                }
            } else {
                quote! {
                    #separator
                    write!(output, "{} -> ", #field_name)?;
                    if let Some(__target_offset) = #crate_path::core::relocation_target_offset(
                        #relocation_kind,
                        <#ty as #crate_path::core::RelocationField>::to_relocation_value(&self.#ident),
                        instruction.offset,
                        instruction.size,
                    ) {
                        write!(output, "{}", __target_offset)?;
                    } else {
                        write!(output, "<invalid>")?;
                    }
                }
            }
        });
        quote!(#(#comments)*)
    };

    let linkable_params = opcode.body.args.iter().map(|field| {
        let ident = &field.ident;
        let ty = &field.ty;
        if field.relocation.is_some() {
            let trait_ident =
                format_ident!("__{}{}IntoReloc", opcode.name.ident, field.variant_ident());
            quote!(#ident: impl #trait_ident)
        } else if field.optional {
            quote!(#ident: Option<#ty>)
        } else {
            quote!(#ident: #ty)
        }
    });

    let linkable_inits = opcode.body.args.iter().map(|field| {
        let ident = &field.ident;
        if field.relocation.is_some() {
            let trait_ident =
                format_ident!("__{}{}IntoReloc", opcode.name.ident, field.variant_ident());
            quote!(#ident: #trait_ident::__into_reloc(#ident),)
        } else {
            quote!(#ident,)
        }
    });

    let linkable_borrow_params = opcode
        .body
        .args
        .iter()
        .map(|field| {
            let ident = &field.ident;
            let ty = &field.ty;
            if field.relocation.is_some() {
                quote!(#ident: &#crate_path::core::RelocValue<#ty>)
            } else if field.optional {
                quote!(#ident: &Option<#ty>)
            } else {
                quote!(#ident: &#ty)
            }
        })
        .collect::<Vec<_>>();
    let mut linkable_borrow_params = linkable_borrow_params;
    if has_metadata {
        linkable_borrow_params.push(quote!(#metadata_id_ident: &u32));
    }

    let mut linkable_value_params = opcode
        .body
        .args
        .iter()
        .map(|field| {
            let ident = &field.ident;
            let ty = &field.ty;
            if field.relocation.is_some() {
                quote!(#ident: #crate_path::core::RelocValue<#ty>)
            } else if field.optional {
                quote!(#ident: Option<#ty>)
            } else {
                quote!(#ident: #ty)
            }
        })
        .collect::<Vec<_>>();
    if has_metadata {
        linkable_value_params.push(quote!(#metadata_id_ident: u32));
    }

    let mut seed_width_checks = opcode
        .body
        .args
        .iter()
        .map(|field| {
            let ident = &field.ident;
            let ty = &field.ty;
            if let Some(_relocation) = field.relocation {
                quote! {
                    && match #ident {
                        #crate_path::core::RelocValue::Value(value) => {
                            <#ty as #crate_path::core::Operand>::fits_width(value, width)
                        }
                        #crate_path::core::RelocValue::Label(_) => {
                            let zero = <#ty as #crate_path::core::RelocationField>::from_relocation_value(0)?;
                            <#ty as #crate_path::core::Operand>::fits_width(&zero, width)
                        }
                    }
                }
            } else if field.optional {
                quote!( && <#ty as #crate_path::core::OptionalOperand>::fits_optional(#ident.as_ref(), width))
            } else {
                quote!( && <#ty as #crate_path::core::Operand>::fits_width(#ident, width))
            }
        })
        .collect::<Vec<_>>();
    if has_metadata {
        seed_width_checks.push(quote!(
            && <u32 as #crate_path::core::Operand>::fits_width(#metadata_id_ident, width)
        ));
    }

    let mut resolved_width_checks = opcode
        .body
        .args
        .iter()
        .map(|field| {
            let ident = &field.ident;
            let ty = &field.ty;
            if let Some(relocation) = field.relocation {
                let relocation_kind = relocation.to_core_tokens(crate_path);
                quote! {
                    && match #ident {
                        #crate_path::core::RelocValue::Value(value) => {
                            <#ty as #crate_path::core::Operand>::fits_width(value, width)
                        }
                        #crate_path::core::RelocValue::Label(label) => {
                            let label_offset = *label_offsets
                                .get(label)
                                .ok_or(#crate_path::core::LinkError::UndefinedLabel(*label))?;
                            let resolved = #relocation_kind
                                .resolve(label_offset, instruction_offset, instruction_size)?;
                            let value = <#ty as #crate_path::core::RelocationField>::from_relocation_value(resolved)?;
                            <#ty as #crate_path::core::Operand>::fits_width(&value, width)
                        }
                    }
                }
            } else if field.optional {
                quote!( && <#ty as #crate_path::core::OptionalOperand>::fits_optional(#ident.as_ref(), width))
            } else {
                quote!( && <#ty as #crate_path::core::Operand>::fits_width(#ident, width))
            }
        })
        .collect::<Vec<_>>();
    if has_metadata {
        resolved_width_checks.push(quote!(
            && <u32 as #crate_path::core::Operand>::fits_width(#metadata_id_ident, width)
        ));
    }

    let mut materialize_fields = opcode
        .body
        .args
        .iter()
        .map(|field| {
            let ident = &field.ident;
            let ty = &field.ty;
            if let Some(relocation) = field.relocation {
                let relocation_kind = relocation.to_core_tokens(crate_path);
                quote! {
                    #ident: match #ident {
                        #crate_path::core::RelocValue::Value(value) => value,
                        #crate_path::core::RelocValue::Label(label) => {
                            let label_offset = *label_offsets
                                .get(&label)
                                .ok_or(#crate_path::core::LinkError::UndefinedLabel(label))?;
                            let resolved = #relocation_kind
                                .resolve(label_offset, instruction_offset, instruction_size)?;
                            <#ty as #crate_path::core::RelocationField>::from_relocation_value(resolved)?
                        }
                    },
                }
            } else {
                quote!(#ident, )
            }
        })
        .collect::<Vec<_>>();
    if has_metadata {
        materialize_fields
            .push(quote!(#metadata_id_ident: #crate_path::core::MetadataId(#metadata_id_ident),));
    }

    let linker_metadata_helpers = if has_metadata {
        let check_args_with_metadata_id = opcode.body.args.iter().map(|field| {
            let ident = &field.ident;
            let ty = &field.ty;
            if field.optional {
                quote!(<#ty as #crate_path::core::OptionalOperand>::fits_optional(self.#ident.as_ref(), width))
            } else {
                quote!(<#ty as #crate_path::core::Operand>::fits_width(&self.#ident, width))
            }
        });

        quote! {
            #[doc(hidden)]
            pub fn __check_width_with_metadata_id(
                &self,
                width: #crate_path::core::OperandWidth,
                metadata_id: u32,
            ) -> bool {
                #crate_path::core::instruction_opcode_fits_width(OpcodeId::#opcode_enum_ident as u16, width)
                    #(&& #check_args_with_metadata_id)*
                    && <u32 as #crate_path::core::Operand>::fits_width(&metadata_id, width)
            }

            #[doc(hidden)]
            pub fn __smallest_width_with_metadata_id(
                &self,
                metadata_id: u32,
            ) -> Result<#crate_path::core::OperandWidth, #crate_path::core::EmitError> {
                for width in #crate_path::core::OperandWidth::in_preference_order() {
                    if Self::__check_width_with_metadata_id(self, width, metadata_id) {
                        return Ok(width);
                    }
                }

                Err(#crate_path::core::EmitError::WidthDoesNotFit {
                    opcode: #emit_error_name,
                    width: #crate_path::core::OperandWidth::Wide32,
                })
            }
        }
    } else {
        quote!()
    };

    let linkable_helpers = if has_relocations {
        let encoded_len_expr = if let Some(layout) = opcode.packed_layout.as_ref() {
            let byte_len = layout.word_count * 4;
            quote! {
                #byte_len
            }
        } else {
            quote! {
                #crate_path::core::instruction_header_len(OpcodeId::#opcode_enum_ident as u16, width)
                    #(+ #encode_len_terms)*
            }
        };

        let linkable_ctor = if opcode.body.metadata.is_empty() {
            let debug_id_init = has_debug.then(|| quote!(debug_id: None,));
            quote! {
                pub fn linkable(#(#linkable_params,)*) -> LinkableInstruction {
                    LinkableInstruction::#reloc_variant_ident {
                        #(#linkable_inits)*
                        #debug_id_init
                    }
                }
            }
        } else {
            let debug_id_init = has_debug.then(|| quote!(debug_id: None,));
            quote! {
                pub fn linkable_with_metadata(
                    #(#linkable_params,)*
                    metadata: #metadata_struct_ident,
                ) -> LinkableInstruction {
                    LinkableInstruction::#reloc_variant_ident {
                        #(#linkable_inits)*
                        metadata,
                        #debug_id_init
                    }
                }
            }
        };

        quote! {
            #linkable_ctor

            #[doc(hidden)]
            pub fn __linkable_encoded_len(width: #crate_path::core::OperandWidth) -> usize {
                #encoded_len_expr
            }

            #[doc(hidden)]
            pub fn __linkable_seed_width(
                #(#linkable_borrow_params,)*
            ) -> Result<#crate_path::core::OperandWidth, #crate_path::core::LinkError> {
                for width in #crate_path::core::OperandWidth::in_preference_order() {
                    if #crate_path::core::instruction_opcode_fits_width(OpcodeId::#opcode_enum_ident as u16, width)
                        #(#seed_width_checks)*
                    {
                        return Ok(width);
                    }
                }

                Err(#crate_path::core::LinkError::Emit(#crate_path::core::EmitError::WidthDoesNotFit {
                    opcode: #emit_error_name,
                    width: #crate_path::core::OperandWidth::Wide32,
                }))
            }

            #[doc(hidden)]
            pub fn __linkable_resolved_width(
                #(#linkable_borrow_params,)*
                instruction_offset: usize,
                instruction_size: usize,
                label_offsets: &std::collections::HashMap<#crate_path::core::Label, usize>,
            ) -> Result<#crate_path::core::OperandWidth, #crate_path::core::LinkError> {
                for width in #crate_path::core::OperandWidth::in_preference_order() {
                    if #crate_path::core::instruction_opcode_fits_width(OpcodeId::#opcode_enum_ident as u16, width)
                        #(#resolved_width_checks)*
                    {
                        return Ok(width);
                    }
                }

                Err(#crate_path::core::LinkError::Emit(#crate_path::core::EmitError::WidthDoesNotFit {
                    opcode: #emit_error_name,
                    width: #crate_path::core::OperandWidth::Wide32,
                }))
            }

            #[doc(hidden)]
            pub fn __materialize_linkable(
                #(#linkable_value_params,)*
                instruction_offset: usize,
                instruction_size: usize,
                label_offsets: &std::collections::HashMap<#crate_path::core::Label, usize>,
            ) -> Result<Self, #crate_path::core::LinkError> {
                Ok(Self {
                    #(#materialize_fields)*
                })
            }
        }
    } else {
        quote!()
    };

    let encode_impl = if is_packed {
        quote! {
            impl #crate_path::core::Encode for #opcode_ident {
                fn encoded_len(&self, _width: #crate_path::core::OperandWidth) -> usize {
                    <#opcode_ident as #crate_path::core::PackedEmit>::packed_word_len(self) * 4
                }

                fn encode_with_width<S: #crate_path::core::ByteSink + ?Sized>(
                    &self,
                    output: &mut S,
                    _width: #crate_path::core::OperandWidth,
                ) {
                    let words = <#opcode_ident as #crate_path::core::PackedEncode>::encode_packed_to_words(self)
                        .expect("packed instruction failed width check");
                    for word in words {
                        output.extend_from_slice(&word.to_le_bytes());
                    }
                }
            }
        }
    } else {
        quote! {
            impl #crate_path::core::Encode for #opcode_ident {
                fn encoded_len(&self, width: #crate_path::core::OperandWidth) -> usize {
                    #crate_path::core::instruction_header_len(OpcodeId::#opcode_enum_ident as u16, width)
                        #(+ #encode_len_terms)*
                }

                fn encode_with_width<S: #crate_path::core::ByteSink + ?Sized>(
                    &self,
                    output: &mut S,
                    width: #crate_path::core::OperandWidth,
                ) {
                    let opcode = OpcodeId::#opcode_enum_ident as u16;
                    assert!(
                        #crate_path::core::instruction_opcode_fits_width(opcode, width),
                        "opcode {} does not fit in {:?} instruction header",
                        stringify!(#opcode_ident),
                        width
                    );
                    #crate_path::core::encode_instruction_header(output, opcode, width);
                    #(#encode_args)*
                }
            }
        }
    };

    let emit_impl = if is_packed {
        quote! {
            impl #crate_path::core::Emit for #opcode_ident {
                fn check_width(&self, _width: #crate_path::core::OperandWidth) -> bool {
                    <#opcode_ident as #crate_path::core::PackedEncode>::encode_packed_to_words(self).is_ok()
                }

                fn emit_with_width<S: #crate_path::core::ByteSink + ?Sized>(
                    &self,
                    output: &mut S,
                    width: #crate_path::core::OperandWidth,
                ) -> Result<(), #crate_path::core::EmitError> {
                    if !<#opcode_ident as #crate_path::core::Emit>::check_width(self, width) {
                        return Err(#crate_path::core::EmitError::WidthDoesNotFit {
                            opcode: #emit_error_name,
                            width,
                        });
                    }

                    <#opcode_ident as #crate_path::core::Encode>::encode_with_width(self, output, width);
                    Ok(())
                }

                fn smallest_width(&self) -> Result<#crate_path::core::OperandWidth, #crate_path::core::EmitError> {
                    if <#opcode_ident as #crate_path::core::Emit>::check_width(
                        self,
                        #crate_path::core::OperandWidth::Narrow,
                    ) {
                        Ok(#crate_path::core::OperandWidth::Narrow)
                    } else {
                        Err(#crate_path::core::EmitError::WidthDoesNotFit {
                            opcode: #emit_error_name,
                            width: #crate_path::core::OperandWidth::Wide32,
                        })
                    }
                }
            }
        }
    } else {
        quote! {
            impl #crate_path::core::Emit for #opcode_ident {
                fn check_width(&self, width: #crate_path::core::OperandWidth) -> bool {
                    #crate_path::core::instruction_opcode_fits_width(OpcodeId::#opcode_enum_ident as u16, width)
                        #(&& #check_args)*
                }

                fn emit_with_width<S: #crate_path::core::ByteSink + ?Sized>(
                    &self,
                    output: &mut S,
                    width: #crate_path::core::OperandWidth,
                ) -> Result<(), #crate_path::core::EmitError> {
                    if !<#opcode_ident as #crate_path::core::Emit>::check_width(self, width) {
                        return Err(#crate_path::core::EmitError::WidthDoesNotFit {
                            opcode: #emit_error_name,
                            width,
                        });
                    }

                    <#opcode_ident as #crate_path::core::Encode>::encode_with_width(self, output, width);
                    Ok(())
                }

                fn smallest_width(&self) -> Result<#crate_path::core::OperandWidth, #crate_path::core::EmitError> {
                    for width in #crate_path::core::OperandWidth::in_preference_order() {
                        if <#opcode_ident as #crate_path::core::Emit>::check_width(self, width) {
                            return Ok(width);
                        }
                    }

                    Err(#crate_path::core::EmitError::WidthDoesNotFit {
                        opcode: #emit_error_name,
                        width: #crate_path::core::OperandWidth::Wide32,
                    })
                }
            }
        }
    };

    let operand_consts: Vec<TokenStream2> = opcode
        .body
        .args
        .iter()
        .enumerate()
        .map(|(i, field)| {
            let upper = field.ident.to_string().to_ascii_uppercase();
            let op_name = format_ident!("OPERAND_{}", upper);
            let sign_name = format_ident!("SIGNED_{}", upper);
            let ty = &field.ty;
            quote! {
                pub const #op_name: usize = #i;
                pub const #sign_name: bool =
                    <#ty as #crate_path::core::Operand>::IS_SIGNED;
            }
        })
        .collect();

    // `metadata_id` encodes last when present.
    let metadata_id_consts = if has_metadata {
        let i = opcode.body.args.len();
        quote! {
            pub const OPERAND_METADATA_ID: usize = #i;
            pub const SIGNED_METADATA_ID: bool = false;
        }
    } else {
        quote! {}
    };

    // `METADATA_<FIELD>` is the field's index in the opcode's metadata struct.
    let metadata_field_consts: Vec<TokenStream2> = opcode
        .body
        .metadata
        .iter()
        .enumerate()
        .map(|(j, field)| {
            let name = format_ident!("METADATA_{}", field.ident.to_string().to_ascii_uppercase());
            quote! {
                pub const #name: usize = #j;
            }
        })
        .collect();

    let metadata_kind_const = metadata_kind.map(|kind| {
        quote! {
            pub const METADATA_KIND: usize = #kind;
        }
    });

    let size_fns = quote! {
        /// Encoded length at narrow, wide16 and wide32, header included.
        pub const LEN: [usize; 3] = [
            Self::size(#crate_path::core::OperandWidth::Narrow),
            Self::size(#crate_path::core::OperandWidth::Wide16),
            Self::size(#crate_path::core::OperandWidth::Wide32),
        ];

        /// Bytes before the first operand: 1 for narrow, 3 for wide16,
        /// 5 for wide32 (a prefix byte plus the opcode at operand width).
        pub const fn header_size(width: #crate_path::core::OperandWidth) -> usize {
            match width {
                #crate_path::core::OperandWidth::Narrow => 1,
                #crate_path::core::OperandWidth::Wide16 => 3,
                #crate_path::core::OperandWidth::Wide32 => 5,
            }
        }

        #[inline(always)]
        pub const fn header_size_static<const WIDTH: u8>() -> usize {
            Self::header_size(#crate_path::core::OperandWidth::from_index(WIDTH))
        }

        /// Total encoded size at `width`.
        pub const fn size(width: #crate_path::core::OperandWidth) -> usize {
            Self::header_size(width) + #args_len * width.bytes()
        }

        #[inline(always)]
        pub const fn size_static<const WIDTH: u8>() -> usize {
            Self::size(#crate_path::core::OperandWidth::from_index(WIDTH))
        }
    };

    let mut ptr_decode_stmts: Vec<TokenStream2> = opcode
        .body
        .args
        .iter()
        .map(|field| {
            let ident = &field.ident;
            let ty = &field.ty;
            if field.optional {
                quote! {
                    debug_assert!(<#ty as #crate_path::core::Operand>::can_decode_from_ptr());
                    let (#ident, ptr) = unsafe {
                        <#ty as #crate_path::core::OptionalOperand>::decode_optional_from_ptr(width, ptr)
                    };
                }
            } else {
                quote! {
                    debug_assert!(<#ty as #crate_path::core::Operand>::can_decode_from_ptr());
                    let (#ident, ptr) = unsafe {
                        <#ty as #crate_path::core::Operand>::decode_from_ptr(width, ptr)
                    };
                }
            }
        })
        .collect();
    let mut ptr_decode_inits: Vec<TokenStream2> = opcode
        .body
        .args
        .iter()
        .map(|field| {
            let ident = &field.ident;
            quote!(#ident,)
        })
        .collect();
    if has_metadata {
        ptr_decode_stmts.push(quote! {
            let (#metadata_id_ident, ptr) = unsafe {
                <#crate_path::core::MetadataId as #crate_path::core::Operand>::decode_from_ptr(width, ptr)
            };
        });
        ptr_decode_inits.push(quote!(#metadata_id_ident,));
    }

    let decode_ptr_fns = quote! {
        /// Decode the operands at `ptr` (past the header) without bounds checks.
        ///
        /// # Safety
        /// `ptr` points at readable bytes holding this instruction's operands
        /// at `width`, and every operand type supports the pointer fast path.
        #[inline(always)]
        pub unsafe fn decode_from_ptr(
            ptr: *const u8,
            width: #crate_path::core::OperandWidth,
        ) -> (Self, *const u8) {
            #(#ptr_decode_stmts)*
            (Self { #(#ptr_decode_inits)* }, ptr)
        }

        /// `decode_from_ptr` with the width as a const index (`OperandWidth::from_index`).
        ///
        /// # Safety
        /// Same as `decode_from_ptr`.
        #[inline(always)]
        pub unsafe fn decode_from_ptr_static<const WIDTH: u8>(
            ptr: *const u8,
        ) -> (Self, *const u8) {
            unsafe {
                Self::decode_from_ptr(ptr, #crate_path::core::OperandWidth::from_index(WIDTH))
            }
        }
    };

    // Flat-area accessor: VMs that keep metadata in a `MetadataLayout` area
    // (instead of the linker's table) read entries through this.
    let metadata_ptr_fn = if has_metadata {
        quote! {
            /// Entry `id` of this opcode's metadata array.
            ///
            /// # Safety
            /// `base` is an initialized flat metadata area with more than
            /// `id` entries of this kind.
            #[inline(always)]
            pub unsafe fn metadata_ptr(
                base: *mut u8,
                id: #crate_path::core::MetadataId,
            ) -> *mut #metadata_struct_ident {
                unsafe {
                    let offset = *(base as *const u32).add(Self::METADATA_KIND);
                    base.add(offset as usize)
                        .cast::<#metadata_struct_ident>()
                        .add(id.0 as usize)
                }
            }
        }
    } else {
        quote! {}
    };

    quote! {
        #(#reloc_input_traits)*
        #metadata_struct

        #opcode_docs
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct #opcode_ident {
            #(#struct_fields)*
        }

        impl #opcode_ident {
            #descriptor_docs
            pub const DESCRIPTOR: OpcodeDescriptor = OpcodeDescriptor {
                id: OpcodeId::#opcode_ident,
                section: SectionId::#section_ident,
                name: #opcode_name,
                mnemonic: #opcode_mnemonic,
                operand_count: #args_len,
                has_metadata: #has_metadata,
                has_relocations: #has_relocations,
            };

            pub fn new(#(#new_params,)*) -> Self {
                Self {
                    #(#new_initializers)*
                }
            }

            #(#operand_consts)*
            #metadata_id_consts
            #(#metadata_field_consts)*
            #metadata_kind_const
            #size_fns
            #decode_ptr_fns
            #metadata_ptr_fn

            #linker_metadata_helpers
            #linkable_helpers
            #metadata_helpers
        }

        #linkable_from
        #tmp_enum
        #checkpoint_enum
        #packed_items

        impl #crate_path::core::InstructionSpec for #opcode_ident {
            type Metadata = #metadata_type;
        }

        #encode_impl

        impl #crate_path::core::Decode for #opcode_ident {
            fn decode_body(
                input: &mut #crate_path::core::ByteCursor<'_>,
                width: #crate_path::core::OperandWidth,
            ) -> Result<Self, #crate_path::core::DecodeError> {
                #(#decode_args)*
                Ok(Self {
                    #(#decode_fields)*
                })
            }
        }

        #emit_impl

        impl<Ctx, Formatter> #crate_path::core::Disassemble<Ctx, Formatter> for #opcode_ident
        where
            #(#disassemble_bounds)*
        {
            fn disassemble_into(
                &self,
                output: &mut dyn std::fmt::Write,
                context: &Ctx,
                formatter: &Formatter,
                instruction: #crate_path::core::InstructionFormatSpec<'_>,
                options: &#crate_path::core::DisassemblerOptions,
            ) -> std::fmt::Result {
                #crate_path::core::write_disassembly_prefix(output, instruction, options)?;
                #(#disassemble_args)*
                #disassemble_reloc_comments
                Ok(())
            }
        }
    }
}
