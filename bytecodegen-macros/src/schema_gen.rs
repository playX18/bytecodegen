use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};

use crate::doc_gen::{combine_doc_attrs, opcode_id_doc_lines, section_doc_lines};
use crate::group_gen::{generate_group_items, generate_section_items};
use crate::hash::schema_macro_suffix;
use crate::interpreter_gen::generate_interpreter_macro_items;
use crate::model::{EncodingModel, SchemaModel};
use crate::opcode_gen::generate_opcode_items;

pub(crate) fn generate_schema(schema: &SchemaModel, crate_path: &TokenStream2) -> TokenStream2 {
    let has_debug = schema.sections.iter().any(|s| s.debug.is_some());
    let debug_model = schema.sections.iter().find_map(|s| s.debug.as_ref());

    let section_variants = schema.sections.iter().map(|section| {
        let ident = &section.name.ident;
        let docs = combine_doc_attrs(&section.docs, section_doc_lines(section));
        quote!(#docs #ident,)
    });

    let section_name_matches = schema.sections.iter().map(|section| {
        let ident = &section.name.ident;
        let name = &section.name.snake_case;
        quote!(SectionId::#ident => #name,)
    });

    let section_kebab_name_matches = schema.sections.iter().map(|section| {
        let ident = &section.name.ident;
        let name = &section.name.kebab_case;
        quote!(SectionId::#ident => #name,)
    });

    let section_opcode_matches = schema.sections.iter().map(|section| {
        let ident = &section.name.ident;
        let opcodes_ident = format_ident!("{}_SECTION_OPCODES", section.name.screaming_snake_case);
        quote!(SectionId::#ident => &#opcodes_ident,)
    });

    let section_source_item_matches = schema.sections.iter().map(|section| {
        let ident = &section.name.ident;
        let items_ident = format_ident!("{}_SECTION_ITEMS", section.name.screaming_snake_case);
        quote!(SectionId::#ident => &#items_ident,)
    });

    let opcode_variants = schema.opcodes.iter().enumerate().map(|(index, opcode)| {
        let ident = &opcode.name.ident;
        let docs = combine_doc_attrs(&opcode.docs, opcode_id_doc_lines(opcode, index as u32));
        quote!(#docs #ident,)
    });

    let opcode_id_idents = schema.opcodes.iter().map(|opcode| &opcode.name.ident);

    let opcode_name_matches = schema.opcodes.iter().map(|opcode| {
        let ident = &opcode.name.ident;
        let name = &opcode.name.snake_case;
        quote!(OpcodeId::#ident => #name,)
    });

    let opcode_kebab_name_matches = schema.opcodes.iter().map(|opcode| {
        let ident = &opcode.name.ident;
        let name = &opcode.name.kebab_case;
        quote!(OpcodeId::#ident => #name,)
    });

    let opcode_section_matches = schema.opcodes.iter().map(|opcode| {
        let ident = &opcode.name.ident;
        let section_ident = &opcode.section.ident;
        quote!(OpcodeId::#ident => SectionId::#section_ident,)
    });

    let opcode_descriptor_matches = schema.opcodes.iter().map(|opcode| {
        let ident = &opcode.name.ident;
        quote!(OpcodeId::#ident => &#ident::DESCRIPTOR,)
    });

    let opcode_items =
        schema.opcodes.iter().enumerate().map(|(index, opcode)| {
            generate_opcode_items(opcode, index as u32, crate_path, has_debug)
        });

    let group_items = schema
        .groups
        .iter()
        .map(|group| generate_group_items(group, crate_path));

    let section_items = schema
        .sections
        .iter()
        .map(|section| generate_section_items(section, crate_path));

    let instruction_variants = schema.opcodes.iter().enumerate().map(|(index, opcode)| {
        let ident = &opcode.name.ident;
        let docs = combine_doc_attrs(&opcode.docs, opcode_id_doc_lines(opcode, index as u32));
        quote!(#docs #ident(#ident),)
    });

    // Generate debug info struct if a debug model is present.
    let debug_struct = debug_model.map(|dm| {
        let name = &dm.name.ident;
        let fields = dm.fields.iter().map(|f| {
            let ident = &f.ident;
            let ty = &f.ty;
            quote!(pub #ident: #ty,)
        });
        quote! {
            #[derive(Clone, PartialEq, Eq, Hash, Debug)]
            pub struct #name {
                #(#fields)*
            }
        }
    });

    // Generate LinkableInstruction variants.
    // When has_debug, add debug_id: Option<DebugId> to each variant except __Bind.
    let linkable_variants = std::iter::once(quote!(__Bind(#crate_path::core::Label),)).chain(
        schema.opcodes.iter().map(|opcode| {
            let ident = &opcode.name.ident;
            let metadata_ident = format_ident!("{}Metadata", opcode.name.ident);
            if !opcode.has_relocations() && opcode.body.metadata.is_empty() {
                if has_debug {
                    quote!(#ident { opcode: #ident, debug_id: Option<#crate_path::core::DebugId> },)
                } else {
                    quote!(#ident(#ident),)
                }
            } else if !opcode.has_relocations() {
                if has_debug {
                    quote!(#ident { opcode: #ident, metadata: #metadata_ident, debug_id: Option<#crate_path::core::DebugId> },)
                } else {
                    quote!(#ident { opcode: #ident, metadata: #metadata_ident },)
                }
            } else {
                let reloc_variant_ident = format_ident!("__{}Linkable", opcode.name.ident);
                let fields = opcode.body.args.iter().map(|field| {
                    let ident = &field.ident;
                    let ty = &field.ty;
                    if field.relocation.is_some() {
                        quote!(#ident: #crate_path::core::RelocValue<#ty>,)
                    } else if field.optional {
                        quote!(#ident: Option<#ty>,)
                    } else {
                        quote!(#ident: #ty,)
                    }
                });
                if opcode.body.metadata.is_empty() {
                    if has_debug {
                        quote!(
                            #ident { opcode: #ident, debug_id: Option<#crate_path::core::DebugId> },
                            #reloc_variant_ident { #(#fields)* debug_id: Option<#crate_path::core::DebugId> },
                        )
                    } else {
                        quote!(
                            #ident(#ident),
                            #reloc_variant_ident { #(#fields)* },
                        )
                    }
                } else {
                    if has_debug {
                        quote!(
                            #ident { opcode: #ident, metadata: #metadata_ident, debug_id: Option<#crate_path::core::DebugId> },
                            #reloc_variant_ident { #(#fields)* metadata: #metadata_ident, debug_id: Option<#crate_path::core::DebugId> },
                        )
                    } else {
                        quote!(
                            #ident { opcode: #ident, metadata: #metadata_ident },
                            #reloc_variant_ident { #(#fields)* metadata: #metadata_ident },
                        )
                    }
                }
            }
        }),
    );

    // Generate with_debug() method when has_debug is true.
    let with_debug_impl = has_debug.then(|| {
        let with_debug_arms = std::iter::once(quote! {
            LinkableInstruction::__Bind(label) => LinkableInstruction::__Bind(label),
        }).chain(schema.opcodes.iter().map(|opcode| {
            let ident = &opcode.name.ident;
            if !opcode.has_relocations() && opcode.body.metadata.is_empty() {
                quote! {
                    LinkableInstruction::#ident { opcode, .. } => LinkableInstruction::#ident { opcode, debug_id: Some(id) },
                }
            } else if !opcode.has_relocations() {
                quote! {
                    LinkableInstruction::#ident { opcode, metadata, .. } => LinkableInstruction::#ident { opcode, metadata, debug_id: Some(id) },
                }
            } else {
                let reloc_variant_ident = format_ident!("__{}Linkable", opcode.name.ident);
                let arg_names = opcode.body.args.iter().map(|f| f.ident.clone()).collect::<Vec<_>>();
                if opcode.body.metadata.is_empty() {
                    quote! {
                        LinkableInstruction::#ident { opcode, .. } => LinkableInstruction::#ident { opcode, debug_id: Some(id) },
                        LinkableInstruction::#reloc_variant_ident { #(#arg_names,)* .. } => LinkableInstruction::#reloc_variant_ident { #(#arg_names,)* debug_id: Some(id) },
                    }
                } else {
                    quote! {
                        LinkableInstruction::#ident { opcode, metadata, .. } => LinkableInstruction::#ident { opcode, metadata, debug_id: Some(id) },
                        LinkableInstruction::#reloc_variant_ident { #(#arg_names,)* metadata, .. } => LinkableInstruction::#reloc_variant_ident { #(#arg_names,)* metadata, debug_id: Some(id) },
                    }
                }
            }
        }));

        quote! {
            impl LinkableInstruction {
                pub fn with_debug(self, id: #crate_path::core::DebugId) -> Self {
                    match self {
                        #(#with_debug_arms)*
                    }
                }
            }
        }
    });

    let visitor_methods = schema.opcodes.iter().map(|opcode| {
        let ident = &opcode.name.ident;
        let visit_ident = format_ident!("visit_{}", opcode.name.snake_case);
        quote!(fn #visit_ident(&mut self, opcode: &#ident) -> Self::Output;)
    });

    let accept_arms = schema.opcodes.iter().map(|opcode| {
        let ident = &opcode.name.ident;
        let visit_ident = format_ident!("visit_{}", opcode.name.snake_case);
        quote!(Self::#ident(opcode) => visitor.#visit_ident(opcode),)
    });

    let instruction_id_arms = schema.opcodes.iter().map(|opcode| {
        let ident = &opcode.name.ident;
        quote!(Self::#ident(_) => OpcodeId::#ident,)
    });

    let instruction_encode_len_arms = schema.opcodes.iter().map(|opcode| {
        let ident = &opcode.name.ident;
        quote!(Self::#ident(opcode) => <#ident as #crate_path::core::Encode>::encoded_len(opcode, width),)
    });

    let instruction_encode_arms = schema.opcodes.iter().map(|opcode| {
        let ident = &opcode.name.ident;
        quote!(Self::#ident(opcode) => <#ident as #crate_path::core::Encode>::encode_with_width(opcode, output, width),)
    });

    let instruction_check_arms = schema.opcodes.iter().map(|opcode| {
        let ident = &opcode.name.ident;
        quote!(Self::#ident(opcode) => <#ident as #crate_path::core::Emit>::check_width(opcode, width),)
    });

    let instruction_emit_arms = schema.opcodes.iter().map(|opcode| {
        let ident = &opcode.name.ident;
        quote!(Self::#ident(opcode) => <#ident as #crate_path::core::Emit>::emit_with_width(opcode, output, width),)
    });

    let instruction_smallest_width_arms = schema.opcodes.iter().map(|opcode| {
        let ident = &opcode.name.ident;
        quote!(Self::#ident(opcode) => <#ident as #crate_path::core::Emit>::smallest_width(opcode),)
    });

    let packed_instruction_word_len_arms = schema.opcodes.iter().map(|opcode| {
        let ident = &opcode.name.ident;
        quote!(Self::#ident(opcode) => <#ident as #crate_path::core::PackedEmit>::packed_word_len(opcode),)
    });

    let packed_instruction_encode_arms = schema.opcodes.iter().map(|opcode| {
        let ident = &opcode.name.ident;
        quote!(Self::#ident(opcode) => <#ident as #crate_path::core::PackedEncode>::encode_packed_to_words(opcode),)
    });

    let packed_instruction_decode_record_arms = schema.opcodes.iter().map(|opcode| {
        let ident = &opcode.name.ident;
        quote! {
            if words.len() == #ident::__packed_layout().word_count {
                matched_layout = true;
                match <#ident as #crate_path::core::PackedDecode>::decode_packed_words(words) {
                    Ok(opcode) => {
                        let raw_words = words.to_vec().into_boxed_slice();
                        let raw_bytes = #crate_path::core::packed_words_to_bytes(words).into_boxed_slice();
                        return Ok(PackedDecodedInstruction {
                            instruction: Instruction::#ident(opcode),
                            word_offset: 0,
                            word_len: words.len(),
                            byte_offset: 0,
                            byte_len: raw_bytes.len(),
                            raw_words,
                            raw_bytes,
                        });
                    }
                    Err(#crate_path::core::DecodeError::InvalidOpcode(opcode)) => {
                        invalid_opcode.get_or_insert(opcode);
                    }
                    Err(error) => return Err(error),
                }
            }
        }
    });

    let packed_instruction_decode_record_from_arms =
        schema.opcodes.iter().enumerate().map(|(index, opcode)| {
            let ident = &opcode.name.ident;
            let opcode_value = index as u32;
            quote! {
                #opcode_value => {
                    let word_len = #ident::__packed_layout().word_count;
                    let end = word_offset + word_len;
                    let slice = words
                        .get(word_offset..end)
                        .ok_or(#crate_path::core::DecodeError::UnexpectedEof)?;
                    let opcode = <#ident as #crate_path::core::PackedDecode>::decode_packed_words(slice)?;
                    let raw_words = slice.to_vec().into_boxed_slice();
                    let raw_bytes = #crate_path::core::packed_words_to_bytes(slice).into_boxed_slice();
                    Ok(PackedDecodedInstruction {
                        instruction: Instruction::#ident(opcode),
                        word_offset,
                        word_len,
                        byte_offset: word_offset * 4,
                        byte_len: raw_bytes.len(),
                        raw_words,
                        raw_bytes,
                    })
                }
            }
        });

    let instruction_disassemble_bounds = schema.opcodes.iter().map(|opcode| {
        let ident = &opcode.name.ident;
        quote!(#ident: #crate_path::core::Disassemble<Ctx, Formatter>,)
    });

    let instruction_disassemble_arms = schema.opcodes.iter().map(|opcode| {
        let ident = &opcode.name.ident;
        quote!(
            Self::#ident(opcode) => <#ident as #crate_path::core::Disassemble<Ctx, Formatter>>::disassemble_into(
                opcode,
                output,
                context,
                formatter,
                instruction,
                options,
            ),
        )
    });

    let instruction_decode_arms = schema.opcodes.iter().enumerate().map(|(index, opcode)| {
        let ident = &opcode.name.ident;
        let opcode_value = index as u32;
        quote!(#opcode_value => Ok(Self::#ident(<#ident as #crate_path::core::Decode>::decode_body(input, width)?)),)
    });

    // Helper: for simple (no-metadata, no-reloc) variants, when has_debug they use
    // struct pattern { opcode, .. }, otherwise tuple pattern (opcode).
    // For metadata variants, they always use { opcode, .. }.
    // For reloc variants, they always use { .. } with named fields.
    // We use .. in patterns that have debug_id to ignore it.

    let link_analysis_arms = std::iter::once(quote!(LinkableInstruction::__Bind(_) => {},)).chain(
        schema
            .opcodes
            .iter()
            .enumerate()
            .flat_map(|(index, opcode)| {
                let ident = &opcode.name.ident;
                let mut arms = Vec::new();

                // Non-reloc simple variant
                if !opcode.has_relocations() && opcode.body.metadata.is_empty() {
                    if has_debug {
                        arms.push(quote! {
                            LinkableInstruction::#ident { opcode, .. } => {
                                let width = <#ident as #crate_path::core::Emit>::smallest_width(opcode)?;
                                code_len = code_len
                                    .checked_add(<#ident as #crate_path::core::Encode>::encoded_len(opcode, width))
                                    .ok_or(#crate_path::core::LinkError::LayoutOverflow("linked code size overflow"))?;
                                widths.push(width);
                            }
                        });
                    } else {
                        arms.push(quote! {
                            LinkableInstruction::#ident(opcode) => {
                                let width = <#ident as #crate_path::core::Emit>::smallest_width(opcode)?;
                                code_len = code_len
                                    .checked_add(<#ident as #crate_path::core::Encode>::encoded_len(opcode, width))
                                    .ok_or(#crate_path::core::LinkError::LayoutOverflow("linked code size overflow"))?;
                                widths.push(width);
                            }
                        });
                    }
                } else if !opcode.has_relocations() {
                    // Metadata-only variant (always struct form)
                    arms.push(quote! {
                        LinkableInstruction::#ident { opcode, .. } => {
                            let slot = metadata_slots[#index];
                            metadata_slots[#index] += 1;
                            let width = #ident::__smallest_width_with_metadata_id(opcode, slot)?;
                            code_len = code_len
                                .checked_add(<#ident as #crate_path::core::Encode>::encoded_len(opcode, width))
                                .ok_or(#crate_path::core::LinkError::LayoutOverflow("linked code size overflow"))?;
                            metadata_counts[#index] = metadata_counts[#index]
                                .checked_add(1)
                                .ok_or(#crate_path::core::LinkError::LayoutOverflow("metadata count overflow"))?;
                            widths.push(width);
                        }
                    });
                } else {
                    // Has relocations - primary variant
                    if opcode.body.metadata.is_empty() {
                        if has_debug {
                            arms.push(quote! {
                                LinkableInstruction::#ident { opcode, .. } => {
                                    let width = <#ident as #crate_path::core::Emit>::smallest_width(opcode)?;
                                    code_len = code_len
                                        .checked_add(<#ident as #crate_path::core::Encode>::encoded_len(opcode, width))
                                        .ok_or(#crate_path::core::LinkError::LayoutOverflow("linked code size overflow"))?;
                                    widths.push(width);
                                }
                            });
                        } else {
                            arms.push(quote! {
                                LinkableInstruction::#ident(opcode) => {
                                    let width = <#ident as #crate_path::core::Emit>::smallest_width(opcode)?;
                                    code_len = code_len
                                        .checked_add(<#ident as #crate_path::core::Encode>::encoded_len(opcode, width))
                                        .ok_or(#crate_path::core::LinkError::LayoutOverflow("linked code size overflow"))?;
                                    widths.push(width);
                                }
                            });
                        }
                    } else {
                        arms.push(quote! {
                            LinkableInstruction::#ident { opcode, .. } => {
                                let slot = metadata_slots[#index];
                                metadata_slots[#index] += 1;
                                let width = #ident::__smallest_width_with_metadata_id(opcode, slot)?;
                                code_len = code_len
                                    .checked_add(<#ident as #crate_path::core::Encode>::encoded_len(opcode, width))
                                    .ok_or(#crate_path::core::LinkError::LayoutOverflow("linked code size overflow"))?;
                                metadata_counts[#index] = metadata_counts[#index]
                                    .checked_add(1)
                                    .ok_or(#crate_path::core::LinkError::LayoutOverflow("metadata count overflow"))?;
                                widths.push(width);
                            }
                        });
                    }
                }

                if opcode.has_relocations() {
                    let reloc_variant_ident = format_ident!("__{}Linkable", opcode.name.ident);
                    let arg_names = opcode
                        .body
                        .args
                        .iter()
                        .map(|field| field.ident.clone())
                        .collect::<Vec<_>>();
                    // Reloc variants always use struct form; with debug, debug_id is ignored via ..
                    if opcode.body.metadata.is_empty() {
                        arms.push(quote! {
                            LinkableInstruction::#reloc_variant_ident { #(#arg_names,)* .. } => {
                                let width = #ident::__linkable_seed_width(#(#arg_names,)*)?;
                                code_len = code_len
                                    .checked_add(#ident::__linkable_encoded_len(width))
                                    .ok_or(#crate_path::core::LinkError::LayoutOverflow("linked code size overflow"))?;
                                widths.push(width);
                            }
                        });
                    } else {
                        arms.push(quote! {
                            LinkableInstruction::#reloc_variant_ident { #(#arg_names,)* .. } => {
                                let slot = metadata_slots[#index];
                                metadata_slots[#index] += 1;
                                let width = #ident::__linkable_seed_width(#(#arg_names,)* &slot,)?;
                                code_len = code_len
                                    .checked_add(#ident::__linkable_encoded_len(width))
                                    .ok_or(#crate_path::core::LinkError::LayoutOverflow("linked code size overflow"))?;
                                metadata_counts[#index] = metadata_counts[#index]
                                    .checked_add(1)
                                    .ok_or(#crate_path::core::LinkError::LayoutOverflow("metadata count overflow"))?;
                                widths.push(width);
                            }
                        });
                    }
                }

                arms
            }),
    );

    let link_layout_arms = std::iter::once(quote! {
        LinkableInstruction::__Bind(label) => {
            if label_offsets.insert(*label, offset).is_some() {
                return Err(#crate_path::core::LinkError::DuplicateLabel(*label));
            }
        }
    })
    .chain(schema.opcodes.iter().flat_map(|opcode| {
        let ident = &opcode.name.ident;
        let mut arms = Vec::new();

        if !opcode.has_relocations() && opcode.body.metadata.is_empty() {
            if has_debug {
                arms.push(quote! {
                    LinkableInstruction::#ident { opcode, .. } => {
                        let width = widths[width_index];
                        let size = <#ident as #crate_path::core::Encode>::encoded_len(opcode, width);
                        offset = offset
                            .checked_add(size)
                            .ok_or(#crate_path::core::LinkError::LayoutOverflow("linked code size overflow"))?;
                        width_index += 1;
                    }
                });
            } else {
                arms.push(quote! {
                    LinkableInstruction::#ident(opcode) => {
                        let width = widths[width_index];
                        let size = <#ident as #crate_path::core::Encode>::encoded_len(opcode, width);
                        offset = offset
                            .checked_add(size)
                            .ok_or(#crate_path::core::LinkError::LayoutOverflow("linked code size overflow"))?;
                        width_index += 1;
                    }
                });
            }
        } else if !opcode.has_relocations() {
            arms.push(quote! {
                LinkableInstruction::#ident { opcode, .. } => {
                    let width = widths[width_index];
                    let size = <#ident as #crate_path::core::Encode>::encoded_len(opcode, width);
                    offset = offset
                        .checked_add(size)
                        .ok_or(#crate_path::core::LinkError::LayoutOverflow("linked code size overflow"))?;
                    width_index += 1;
                }
            });
        } else {
            if opcode.body.metadata.is_empty() {
                if has_debug {
                    arms.push(quote! {
                        LinkableInstruction::#ident { opcode, .. } => {
                            let width = widths[width_index];
                            let size = <#ident as #crate_path::core::Encode>::encoded_len(opcode, width);
                            offset = offset
                                .checked_add(size)
                                .ok_or(#crate_path::core::LinkError::LayoutOverflow("linked code size overflow"))?;
                            width_index += 1;
                        }
                    });
                } else {
                    arms.push(quote! {
                        LinkableInstruction::#ident(opcode) => {
                            let width = widths[width_index];
                            let size = <#ident as #crate_path::core::Encode>::encoded_len(opcode, width);
                            offset = offset
                                .checked_add(size)
                                .ok_or(#crate_path::core::LinkError::LayoutOverflow("linked code size overflow"))?;
                            width_index += 1;
                        }
                    });
                }
            } else {
                arms.push(quote! {
                    LinkableInstruction::#ident { opcode, .. } => {
                        let width = widths[width_index];
                        let size = <#ident as #crate_path::core::Encode>::encoded_len(opcode, width);
                        offset = offset
                            .checked_add(size)
                            .ok_or(#crate_path::core::LinkError::LayoutOverflow("linked code size overflow"))?;
                        width_index += 1;
                    }
                });
            }
        }

        if opcode.has_relocations() {
            let reloc_variant_ident = format_ident!("__{}Linkable", opcode.name.ident);
            arms.push(quote! {
                LinkableInstruction::#reloc_variant_ident { .. } => {
                    let width = widths[width_index];
                    let size = #ident::__linkable_encoded_len(width);
                    offset = offset
                        .checked_add(size)
                        .ok_or(#crate_path::core::LinkError::LayoutOverflow("linked code size overflow"))?;
                    width_index += 1;
                }
            });
        }

        arms
    }));

    let link_fixpoint_arms = std::iter::once(quote!(LinkableInstruction::__Bind(_) => {},)).chain(
        schema
            .opcodes
            .iter()
            .enumerate()
            .flat_map(|(index, opcode)| {
                let ident = &opcode.name.ident;
                let mut arms = Vec::new();

                if !opcode.has_relocations() && opcode.body.metadata.is_empty() {
                    if has_debug {
                        arms.push(quote! {
                            LinkableInstruction::#ident { opcode, .. } => {
                                let width = widths[width_index];
                                let size = <#ident as #crate_path::core::Encode>::encoded_len(opcode, width);
                                offset += size;
                                width_index += 1;
                            }
                        });
                    } else {
                        arms.push(quote! {
                            LinkableInstruction::#ident(opcode) => {
                                let width = widths[width_index];
                                let size = <#ident as #crate_path::core::Encode>::encoded_len(opcode, width);
                                offset += size;
                                width_index += 1;
                            }
                        });
                    }
                } else if !opcode.has_relocations() {
                    arms.push(quote! {
                        LinkableInstruction::#ident { opcode, .. } => {
                            metadata_slots[#index] += 1;
                            let width = widths[width_index];
                            let size = <#ident as #crate_path::core::Encode>::encoded_len(opcode, width);
                            offset += size;
                            width_index += 1;
                        }
                    });
                } else {
                    if opcode.body.metadata.is_empty() {
                        if has_debug {
                            arms.push(quote! {
                                LinkableInstruction::#ident { opcode, .. } => {
                                    let width = widths[width_index];
                                    let size = <#ident as #crate_path::core::Encode>::encoded_len(opcode, width);
                                    offset += size;
                                    width_index += 1;
                                }
                            });
                        } else {
                            arms.push(quote! {
                                LinkableInstruction::#ident(opcode) => {
                                    let width = widths[width_index];
                                    let size = <#ident as #crate_path::core::Encode>::encoded_len(opcode, width);
                                    offset += size;
                                    width_index += 1;
                                }
                            });
                        }
                    } else {
                        arms.push(quote! {
                            LinkableInstruction::#ident { opcode, .. } => {
                                metadata_slots[#index] += 1;
                                let width = widths[width_index];
                                let size = <#ident as #crate_path::core::Encode>::encoded_len(opcode, width);
                                offset += size;
                                width_index += 1;
                            }
                        });
                    }
                }

                if opcode.has_relocations() {
                    let reloc_variant_ident = format_ident!("__{}Linkable", opcode.name.ident);
                    let arg_names = opcode
                        .body
                        .args
                        .iter()
                        .map(|field| field.ident.clone())
                        .collect::<Vec<_>>();
                    if opcode.body.metadata.is_empty() {
                        arms.push(quote! {
                            LinkableInstruction::#reloc_variant_ident { #(#arg_names,)* .. } => {
                                let width = widths[width_index];
                                let size = #ident::__linkable_encoded_len(width);
                                let required = #ident::__linkable_resolved_width(
                                    #(#arg_names,)*
                                    offset,
                                    size,
                                    &label_offsets,
                                )?;
                                if required.bytes() > width.bytes() {
                                    widths[width_index] = required;
                                    changed = true;
                                }
                                offset += size;
                                width_index += 1;
                            }
                        });
                    } else {
                        arms.push(quote! {
                            LinkableInstruction::#reloc_variant_ident { #(#arg_names,)* .. } => {
                                let slot = metadata_slots[#index];
                                metadata_slots[#index] += 1;
                                let width = widths[width_index];
                                let size = #ident::__linkable_encoded_len(width);
                                let required = #ident::__linkable_resolved_width(
                                    #(#arg_names,)*
                                    &slot,
                                    offset,
                                    size,
                                    &label_offsets,
                                )?;
                                if required.bytes() > width.bytes() {
                                    widths[width_index] = required;
                                    changed = true;
                                }
                                offset += size;
                                width_index += 1;
                            }
                        });
                    }
                }

                arms
            }),
    );

    // Materialize arms: extract debug_id when has_debug and collect debug_ranges.
    let link_materialize_arms = std::iter::once(quote!(LinkableInstruction::__Bind(_) => {},)).chain(
        schema
            .opcodes
            .iter()
            .enumerate()
            .flat_map(|(index, opcode)| {
                let ident = &opcode.name.ident;
                let metadata_ident = format_ident!("{}Metadata", opcode.name.ident);
                let mut arms = Vec::new();

                if !opcode.has_relocations() && opcode.body.metadata.is_empty() {
                    // Simple variant
                    if has_debug {
                        arms.push(quote! {
                            LinkableInstruction::#ident { opcode, debug_id } => {
                                let size = <#ident as #crate_path::core::Encode>::encoded_len(&opcode, width);
                                let end = offset + size;
                                let mut sink = #crate_path::core::SliceSink::new(&mut code_bytes[offset..end]);
                                <#ident as #crate_path::core::Encode>::encode_with_width(&opcode, &mut sink, width);
                                debug_assert_eq!(sink.position(), size);
                                *entry = #crate_path::core::LinkedInstructionEntry::without_metadata(offset as u32);
                                if let Some(id) = debug_id {
                                    debug_ranges.push((offset..end, id));
                                }
                                offset = end;
                            }
                        });
                    } else {
                        arms.push(quote! {
                            LinkableInstruction::#ident(opcode) => {
                                let size = <#ident as #crate_path::core::Encode>::encoded_len(&opcode, width);
                                let end = offset + size;
                                let mut sink = #crate_path::core::SliceSink::new(&mut code_bytes[offset..end]);
                                <#ident as #crate_path::core::Encode>::encode_with_width(&opcode, &mut sink, width);
                                debug_assert_eq!(sink.position(), size);
                                *entry = #crate_path::core::LinkedInstructionEntry::without_metadata(offset as u32);
                                offset = end;
                            }
                        });
                    }
                } else if !opcode.has_relocations() {
                    // Metadata-only variant
                    if has_debug {
                        arms.push(quote! {
                            LinkableInstruction::#ident { opcode, metadata, debug_id } => {
                                let slot = metadata_slots[#index];
                                metadata_slots[#index] += 1;
                                let mut opcode = opcode;
                                opcode.metadata_id = slot;
                                let size = <#ident as #crate_path::core::Encode>::encoded_len(&opcode, width);
                                let end = offset + size;
                                let mut sink = #crate_path::core::SliceSink::new(&mut code_bytes[offset..end]);
                                <#ident as #crate_path::core::Encode>::encode_with_width(&opcode, &mut sink, width);
                                debug_assert_eq!(sink.position(), size);
                                *entry = #crate_path::core::LinkedInstructionEntry::without_metadata(offset as u32);
                                let header = metadata_layout.headers[#index];
                                let ptr = unsafe {
                                    #crate_path::core::metadata_slot_ptr::<#metadata_ident>(
                                        metadata_bytes,
                                        header,
                                        slot,
                                    )
                                };
                                unsafe {
                                    ptr.write(metadata);
                                }
                                if let Some(id) = debug_id {
                                    debug_ranges.push((offset..end, id));
                                }
                                offset = end;
                            }
                        });
                    } else {
                        arms.push(quote! {
                            LinkableInstruction::#ident { opcode, metadata } => {
                                let slot = metadata_slots[#index];
                                metadata_slots[#index] += 1;
                                let mut opcode = opcode;
                                opcode.metadata_id = slot;
                                let size = <#ident as #crate_path::core::Encode>::encoded_len(&opcode, width);
                                let end = offset + size;
                                let mut sink = #crate_path::core::SliceSink::new(&mut code_bytes[offset..end]);
                                <#ident as #crate_path::core::Encode>::encode_with_width(&opcode, &mut sink, width);
                                debug_assert_eq!(sink.position(), size);
                                *entry = #crate_path::core::LinkedInstructionEntry::without_metadata(offset as u32);
                                let header = metadata_layout.headers[#index];
                                let ptr = unsafe {
                                    #crate_path::core::metadata_slot_ptr::<#metadata_ident>(
                                        metadata_bytes,
                                        header,
                                        slot,
                                    )
                                };
                                unsafe {
                                    ptr.write(metadata);
                                }
                                offset = end;
                            }
                        });
                    }
                } else {
                    // Has relocations - primary variant
                    if opcode.body.metadata.is_empty() {
                        if has_debug {
                            arms.push(quote! {
                                LinkableInstruction::#ident { opcode, debug_id } => {
                                    let size = <#ident as #crate_path::core::Encode>::encoded_len(&opcode, width);
                                    let end = offset + size;
                                    let mut sink = #crate_path::core::SliceSink::new(&mut code_bytes[offset..end]);
                                    <#ident as #crate_path::core::Encode>::encode_with_width(&opcode, &mut sink, width);
                                    debug_assert_eq!(sink.position(), size);
                                    *entry = #crate_path::core::LinkedInstructionEntry::without_metadata(offset as u32);
                                    if let Some(id) = debug_id {
                                        debug_ranges.push((offset..end, id));
                                    }
                                    offset = end;
                                }
                            });
                        } else {
                            arms.push(quote! {
                                LinkableInstruction::#ident(opcode) => {
                                    let size = <#ident as #crate_path::core::Encode>::encoded_len(&opcode, width);
                                    let end = offset + size;
                                    let mut sink = #crate_path::core::SliceSink::new(&mut code_bytes[offset..end]);
                                    <#ident as #crate_path::core::Encode>::encode_with_width(&opcode, &mut sink, width);
                                    debug_assert_eq!(sink.position(), size);
                                    *entry = #crate_path::core::LinkedInstructionEntry::without_metadata(offset as u32);
                                    offset = end;
                                }
                            });
                        }
                    } else {
                        if has_debug {
                            arms.push(quote! {
                                LinkableInstruction::#ident { opcode, metadata, debug_id } => {
                                    let slot = metadata_slots[#index];
                                    metadata_slots[#index] += 1;
                                    let mut opcode = opcode;
                                    opcode.metadata_id = slot;
                                    let size = <#ident as #crate_path::core::Encode>::encoded_len(&opcode, width);
                                    let end = offset + size;
                                    let mut sink = #crate_path::core::SliceSink::new(&mut code_bytes[offset..end]);
                                    <#ident as #crate_path::core::Encode>::encode_with_width(&opcode, &mut sink, width);
                                    debug_assert_eq!(sink.position(), size);
                                    *entry = #crate_path::core::LinkedInstructionEntry::without_metadata(offset as u32);
                                    let header = metadata_layout.headers[#index];
                                    let ptr = unsafe {
                                        #crate_path::core::metadata_slot_ptr::<#metadata_ident>(
                                            metadata_bytes,
                                            header,
                                            slot,
                                        )
                                    };
                                    unsafe {
                                        ptr.write(metadata);
                                    }
                                    if let Some(id) = debug_id {
                                        debug_ranges.push((offset..end, id));
                                    }
                                    offset = end;
                                }
                            });
                        } else {
                            arms.push(quote! {
                                LinkableInstruction::#ident { opcode, metadata } => {
                                    let slot = metadata_slots[#index];
                                    metadata_slots[#index] += 1;
                                    let mut opcode = opcode;
                                    opcode.metadata_id = slot;
                                    let size = <#ident as #crate_path::core::Encode>::encoded_len(&opcode, width);
                                    let end = offset + size;
                                    let mut sink = #crate_path::core::SliceSink::new(&mut code_bytes[offset..end]);
                                    <#ident as #crate_path::core::Encode>::encode_with_width(&opcode, &mut sink, width);
                                    debug_assert_eq!(sink.position(), size);
                                    *entry = #crate_path::core::LinkedInstructionEntry::without_metadata(offset as u32);
                                    let header = metadata_layout.headers[#index];
                                    let ptr = unsafe {
                                        #crate_path::core::metadata_slot_ptr::<#metadata_ident>(
                                            metadata_bytes,
                                            header,
                                            slot,
                                        )
                                    };
                                    unsafe {
                                        ptr.write(metadata);
                                    }
                                    offset = end;
                                }
                            });
                        }
                    }
                }

                if opcode.has_relocations() {
                    let reloc_variant_ident = format_ident!("__{}Linkable", opcode.name.ident);
                    let arg_names = opcode
                        .body
                        .args
                        .iter()
                        .map(|field| field.ident.clone())
                        .collect::<Vec<_>>();
                    if opcode.body.metadata.is_empty() {
                        if has_debug {
                            arms.push(quote! {
                                LinkableInstruction::#reloc_variant_ident { #(#arg_names,)* debug_id, .. } => {
                                    let size = #ident::__linkable_encoded_len(width);
                                    let end = offset + size;
                                    let opcode = #ident::__materialize_linkable(
                                        #(#arg_names,)*
                                        offset,
                                        size,
                                        &label_offsets,
                                    )?;
                                    let mut sink = #crate_path::core::SliceSink::new(&mut code_bytes[offset..end]);
                                    <#ident as #crate_path::core::Encode>::encode_with_width(&opcode, &mut sink, width);
                                    debug_assert_eq!(sink.position(), size);
                                    *entry = #crate_path::core::LinkedInstructionEntry::without_metadata(offset as u32);
                                    if let Some(id) = debug_id {
                                        debug_ranges.push((offset..end, id));
                                    }
                                    offset = end;
                                }
                            });
                        } else {
                            arms.push(quote! {
                                LinkableInstruction::#reloc_variant_ident { #(#arg_names,)* } => {
                                    let size = #ident::__linkable_encoded_len(width);
                                    let end = offset + size;
                                    let opcode = #ident::__materialize_linkable(
                                        #(#arg_names,)*
                                        offset,
                                        size,
                                        &label_offsets,
                                    )?;
                                    let mut sink = #crate_path::core::SliceSink::new(&mut code_bytes[offset..end]);
                                    <#ident as #crate_path::core::Encode>::encode_with_width(&opcode, &mut sink, width);
                                    debug_assert_eq!(sink.position(), size);
                                    *entry = #crate_path::core::LinkedInstructionEntry::without_metadata(offset as u32);
                                    offset = end;
                                }
                            });
                        }
                    } else {
                        if has_debug {
                            arms.push(quote! {
                                LinkableInstruction::#reloc_variant_ident { #(#arg_names,)* metadata, debug_id, .. } => {
                                    let slot = metadata_slots[#index];
                                    metadata_slots[#index] += 1;
                                    let size = #ident::__linkable_encoded_len(width);
                                    let end = offset + size;
                                    let opcode = #ident::__materialize_linkable(
                                        #(#arg_names,)*
                                        slot,
                                        offset,
                                        size,
                                        &label_offsets,
                                    )?;
                                    let mut sink = #crate_path::core::SliceSink::new(&mut code_bytes[offset..end]);
                                    <#ident as #crate_path::core::Encode>::encode_with_width(&opcode, &mut sink, width);
                                    debug_assert_eq!(sink.position(), size);
                                    *entry = #crate_path::core::LinkedInstructionEntry::without_metadata(offset as u32);
                                    let header = metadata_layout.headers[#index];
                                    let ptr = unsafe {
                                        #crate_path::core::metadata_slot_ptr::<#metadata_ident>(
                                            metadata_bytes,
                                            header,
                                            slot,
                                        )
                                    };
                                    unsafe {
                                        ptr.write(metadata);
                                    }
                                    if let Some(id) = debug_id {
                                        debug_ranges.push((offset..end, id));
                                    }
                                    offset = end;
                                }
                            });
                        } else {
                            arms.push(quote! {
                                LinkableInstruction::#reloc_variant_ident { #(#arg_names,)* metadata } => {
                                    let slot = metadata_slots[#index];
                                    metadata_slots[#index] += 1;
                                    let size = #ident::__linkable_encoded_len(width);
                                    let end = offset + size;
                                    let opcode = #ident::__materialize_linkable(
                                        #(#arg_names,)*
                                        slot,
                                        offset,
                                        size,
                                        &label_offsets,
                                    )?;
                                    let mut sink = #crate_path::core::SliceSink::new(&mut code_bytes[offset..end]);
                                    <#ident as #crate_path::core::Encode>::encode_with_width(&opcode, &mut sink, width);
                                    debug_assert_eq!(sink.position(), size);
                                    *entry = #crate_path::core::LinkedInstructionEntry::without_metadata(offset as u32);
                                    let header = metadata_layout.headers[#index];
                                    let ptr = unsafe {
                                        #crate_path::core::metadata_slot_ptr::<#metadata_ident>(
                                            metadata_bytes,
                                            header,
                                            slot,
                                        )
                                    };
                                    unsafe {
                                        ptr.write(metadata);
                                    }
                                    offset = end;
                                }
                            });
                        }
                    }
                }

                arms
            }),
    );

    let metadata_type_entries = schema.opcodes.iter().map(|opcode| {
        let metadata_ident = format_ident!("{}Metadata", opcode.name.ident);
        if opcode.body.metadata.is_empty() {
            quote!(#crate_path::core::MetadataTypeSpec::of::<()>("()"),)
        } else {
            quote!(#crate_path::core::MetadataTypeSpec::of::<#metadata_ident>(stringify!(#metadata_ident)),)
        }
    });

    let schema_opcode_len = schema.opcodes.len();
    let macro_suffix = schema_macro_suffix(schema);
    let packed_schema_items = match schema.encoding {
        EncodingModel::Packed { .. } => quote! {
            pub struct PackedDecodedInstruction {
                pub instruction: Instruction,
                pub word_offset: usize,
                pub word_len: usize,
                pub byte_offset: usize,
                pub byte_len: usize,
                pub raw_words: Box<[u32]>,
                pub raw_bytes: Box<[u8]>,
            }

            impl Instruction {
                pub fn encode_packed_to_words(&self) -> Result<Vec<u32>, #crate_path::core::EmitError> {
                    <Self as #crate_path::core::PackedEncode>::encode_packed_to_words(self)
                }

                pub fn encode_packed_to_bytes(&self) -> Result<Vec<u8>, #crate_path::core::EmitError> {
                    let words = self.encode_packed_to_words()?;
                    Ok(#crate_path::core::packed_words_to_bytes(&words))
                }

                pub fn decode_packed_record(
                    words: &[u32],
                ) -> Result<PackedDecodedInstruction, #crate_path::core::DecodeError> {
                    let mut matched_layout = false;
                    let mut invalid_opcode = None;
                    #(#packed_instruction_decode_record_arms)*

                    if let Some(opcode) = invalid_opcode {
                        return Err(#crate_path::core::DecodeError::InvalidOpcode(opcode));
                    }

                    Err(#crate_path::core::DecodeError::InvalidValue(if matched_layout {
                        "packed words did not match any generated opcode"
                    } else {
                        "packed word input length must match a generated opcode layout"
                    }))
                }

                pub fn decode_packed_record_from(
                    words: &[u32],
                    word_offset: usize,
                ) -> Result<PackedDecodedInstruction, #crate_path::core::DecodeError> {
                    let opcode = words
                        .get(word_offset)
                        .copied()
                        .map(|word| #crate_path::core::extract_packed_bits(word, 0, 8))
                        .ok_or(#crate_path::core::DecodeError::UnexpectedEof)?;

                    match opcode {
                        #(#packed_instruction_decode_record_from_arms,)*
                        _ => Err(#crate_path::core::DecodeError::InvalidOpcode(opcode)),
                    }
                }

                pub fn decode_packed_words(
                    words: &[u32],
                ) -> Result<Self, #crate_path::core::DecodeError> {
                    Ok(Self::decode_packed_record(words)?.instruction)
                }

                pub fn decode_packed_bytes(
                    bytes: &[u8],
                ) -> Result<Self, #crate_path::core::DecodeError> {
                    let words = #crate_path::core::packed_bytes_to_words(bytes)?;
                    Self::decode_packed_words(&words)
                }
            }

            impl PackedDecodedInstruction {
                fn format_spec_with_bytes_width(
                    &self,
                    ordinal: usize,
                    bytes_column_width: usize,
                ) -> #crate_path::core::InstructionFormatSpec<'_> {
                    #crate_path::core::InstructionFormatSpec {
                        ordinal,
                        offset: self.byte_offset,
                        size: self.byte_len,
                        width: #crate_path::core::OperandWidth::Wide32,
                        opcode_name: self.instruction.opcode_id().name(),
                        raw_bytes: &self.raw_bytes,
                        raw_words: Some(&self.raw_words),
                        bytes_column_width,
                    }
                }

                pub fn format_spec(
                    &self,
                    ordinal: usize,
                ) -> #crate_path::core::InstructionFormatSpec<'_> {
                    let bytes_column_width = #crate_path::core::rendered_words_width(&self.raw_words);
                    self.format_spec_with_bytes_width(ordinal, bytes_column_width)
                }

                fn disassemble_into_with_bytes_width<Ctx, Formatter>(
                    &self,
                    output: &mut dyn std::fmt::Write,
                    ordinal: usize,
                    bytes_column_width: usize,
                    context: &Ctx,
                    formatter: &Formatter,
                    options: &#crate_path::core::DisassemblerOptions,
                ) -> Result<(), #crate_path::core::DisassembleError>
                where
                    Instruction: #crate_path::core::Disassemble<Ctx, Formatter>,
                {
                    let instruction = self.format_spec_with_bytes_width(ordinal, bytes_column_width);
                    <Instruction as #crate_path::core::Disassemble<Ctx, Formatter>>::disassemble_into(
                        &self.instruction,
                        output,
                        context,
                        formatter,
                        instruction,
                        options,
                    )
                    .map_err(#crate_path::core::DisassembleError::from)
                }

                pub fn disassemble_into<Ctx, Formatter>(
                    &self,
                    output: &mut dyn std::fmt::Write,
                    ordinal: usize,
                    context: &Ctx,
                    formatter: &Formatter,
                    options: &#crate_path::core::DisassemblerOptions,
                ) -> Result<(), #crate_path::core::DisassembleError>
                where
                    Instruction: #crate_path::core::Disassemble<Ctx, Formatter>,
                {
                    let bytes_column_width = if options.show_bytes {
                        #crate_path::core::rendered_words_width(&self.raw_words)
                    } else {
                        0
                    };
                    self.disassemble_into_with_bytes_width(
                        output,
                        ordinal,
                        bytes_column_width,
                        context,
                        formatter,
                        options,
                    )
                }

                pub fn disassemble_with<Ctx, Formatter>(
                    &self,
                    ordinal: usize,
                    context: &Ctx,
                    formatter: &Formatter,
                    options: &#crate_path::core::DisassemblerOptions,
                ) -> Result<String, #crate_path::core::DisassembleError>
                where
                    Instruction: #crate_path::core::Disassemble<Ctx, Formatter>,
                {
                    let mut output = String::new();
                    self.disassemble_into(&mut output, ordinal, context, formatter, options)?;
                    Ok(output)
                }
            }

            impl #crate_path::core::PackedEncode for Instruction {
                fn encode_packed_to_words(&self) -> Result<Vec<u32>, #crate_path::core::EmitError> {
                    match self {
                        #(#packed_instruction_encode_arms)*
                    }
                }
            }

            impl #crate_path::core::PackedDecode for Instruction {
                fn decode_packed_words(words: &[u32]) -> Result<Self, #crate_path::core::DecodeError> {
                    Self::decode_packed_words(words)
                }
            }

            impl #crate_path::core::PackedEmit for Instruction {
                fn packed_word_len(&self) -> usize {
                    match self {
                        #(#packed_instruction_word_len_arms)*
                    }
                }
            }

            pub fn disassemble_packed_words<Ctx>(
                words: &[u32],
                context: &Ctx,
                options: &#crate_path::core::DisassemblerOptions,
            ) -> Result<String, #crate_path::core::DisassembleError>
            where
                Instruction: #crate_path::core::Disassemble<
                    Ctx,
                    #crate_path::core::DefaultOperandFormatter,
                >,
            {
                let formatter = #crate_path::core::DefaultOperandFormatter;
                disassemble_packed_words_with(words, context, &formatter, options)
            }

            pub fn disassemble_packed_words_with<Ctx, Formatter>(
                words: &[u32],
                context: &Ctx,
                formatter: &Formatter,
                options: &#crate_path::core::DisassemblerOptions,
            ) -> Result<String, #crate_path::core::DisassembleError>
            where
                Instruction: #crate_path::core::Disassemble<Ctx, Formatter>,
            {
                let mut word_offset = 0usize;
                let mut decoded = Vec::new();

                while word_offset < words.len() {
                    let instruction = Instruction::decode_packed_record_from(words, word_offset)?;
                    word_offset += instruction.word_len;
                    decoded.push(instruction);
                }

                let bytes_column_width = if options.show_bytes {
                    decoded
                        .iter()
                        .map(|instruction| {
                            #crate_path::core::rendered_words_width(&instruction.raw_words)
                        })
                        .max()
                        .unwrap_or(0)
                } else {
                    0
                };

                let mut output = String::new();
                for (ordinal, instruction) in decoded.iter().enumerate() {
                    if ordinal > 0 {
                        output.push('\n');
                    }

                    instruction.disassemble_into_with_bytes_width(
                        &mut output,
                        ordinal,
                        bytes_column_width,
                        context,
                        formatter,
                        options,
                    )?;
                }

                Ok(output)
            }

            pub fn disassemble_packed_bytes<Ctx>(
                bytes: &[u8],
                context: &Ctx,
                options: &#crate_path::core::DisassemblerOptions,
            ) -> Result<String, #crate_path::core::DisassembleError>
            where
                Instruction: #crate_path::core::Disassemble<
                    Ctx,
                    #crate_path::core::DefaultOperandFormatter,
                >,
            {
                let formatter = #crate_path::core::DefaultOperandFormatter;
                disassemble_packed_bytes_with(bytes, context, &formatter, options)
            }

            pub fn disassemble_packed_bytes_with<Ctx, Formatter>(
                bytes: &[u8],
                context: &Ctx,
                formatter: &Formatter,
                options: &#crate_path::core::DisassemblerOptions,
            ) -> Result<String, #crate_path::core::DisassembleError>
            where
                Instruction: #crate_path::core::Disassemble<Ctx, Formatter>,
            {
                let words = #crate_path::core::packed_bytes_to_words(bytes)?;
                disassemble_packed_words_with(&words, context, formatter, options)
            }
        },
        EncodingModel::Bytes => quote!(),
    };
    let interpreter_macro_items =
        generate_interpreter_macro_items(schema, crate_path, &macro_suffix);

    // Generate verification code if any opcode has a #[verify] annotation.
    let has_verify = schema.opcodes.iter().any(|op| op.verify.is_some());
    let verify_code = if has_verify {
        // For opcodes with #[verify(stack_balance = N)], generate arms that adjust
        // and check the stack height.
        let stack_balance_arms: Vec<TokenStream2> = schema.opcodes.iter()
            .filter_map(|op| {
                match &op.verify {
                    Some(verify) => match &verify.kind {
                        crate::model::VerifyKind::StackBalance(n) => {
                            let ident = &op.name.ident;
                            let delta_val = *n;
                            Some(quote! {
                                OpcodeId::#ident => {
                                    let delta: i32 = #delta_val;
                                    if state.stack_height + delta < 0 {
                                        errors.push(#crate_path::core::VerificationError::new(
                                            ordinal,
                                            format!("stack underflow: stack_height {} + delta {} < 0", state.stack_height, delta),
                                        ));
                                    }
                                    state.stack_height += delta;
                                }
                            })
                        }
                        _ => None,
                    },
                    None => None,
                }
            })
            .collect();

        // For opcodes with #[verify(jump_target_valid)], generate arms that
        // decode the instruction and check that any relocatable operand values
        // point to valid instruction boundaries (offsets in the ordinals table).
        let jump_target_arms: Vec<TokenStream2> = schema.opcodes.iter()
            .filter_map(|op| {
                match &op.verify {
                    Some(verify) => match &verify.kind {
                        crate::model::VerifyKind::JumpTargetValid => {
                            let ident = &op.name.ident;
                            let reloc_fields: Vec<_> = op.body.args.iter()
                                .filter(|f| f.relocation.is_some())
                                .collect();
                            if reloc_fields.is_empty() {
                                // No relocatable fields; nothing to validate.
                                Some(quote! {
                                    OpcodeId::#ident => {}
                                })
                            } else {
                                // Decode the instruction body (header already decoded above),
                                // then check each relocatable field's value against the
                                // ordinals table.
                                let field_checks = reloc_fields.iter().map(|field| {
                                    let field_ident = &field.ident;
                                    quote! {
                                        let target_offset = decoded.#field_ident as usize;
                                        if !valid_offsets.contains(&target_offset) {
                                            errors.push(#crate_path::core::VerificationError::new(
                                                ordinal,
                                                format!("jump target offset {} is not a valid instruction boundary", target_offset),
                                            ));
                                        }
                                    }
                                });
                                Some(quote! {
                                    OpcodeId::#ident => {
                                        let decoded = #ident::decode_body(&mut cursor, width).map_err(|e| {
                                            vec![#crate_path::core::VerificationError::new(
                                                ordinal,
                                                format!("decode error at instruction {}: {:?}", ordinal, e),
                                            )]
                                        })?;
                                        #(#field_checks)*
                                    }
                                })
                            }
                        }
                        _ => None,
                    },
                    None => None,
                }
            })
            .collect();

        // For opcodes with #[verify(custom(...))], generate no-op arms (future extension).
        let custom_arms: Vec<TokenStream2> = schema
            .opcodes
            .iter()
            .filter_map(|op| match &op.verify {
                Some(verify) => match &verify.kind {
                    crate::model::VerifyKind::Custom(_) => {
                        let ident = &op.name.ident;
                        Some(quote! {
                            OpcodeId::#ident => {}
                        })
                    }
                    _ => None,
                },
                None => None,
            })
            .collect();

        quote! {
            #[derive(Debug, Clone, Default)]
            pub struct VerificationContext {
                pub stack_height: i32,
            }

            pub fn verify<V: #crate_path::core::LinkedBytecodeView + ?Sized>(
                bytecode: &V,
            ) -> #crate_path::core::VerificationResult {
                let mut state = VerificationContext::default();
                let mut errors: Vec<#crate_path::core::VerificationError> = Vec::new();
                let mut ordinal_iter = bytecode.instructions().iter().enumerate();
                // Collect valid instruction offsets for jump target validation.
                let valid_offsets: std::collections::HashSet<usize> = bytecode.instructions().iter()
                    .map(|e| e.offset as usize)
                    .collect();

                while let Some((ordinal, entry)) = ordinal_iter.next() {
                    let offset = entry.offset as usize;
                    let entry_bytes = &bytecode.bytes()[offset..];
                    let mut cursor = #crate_path::core::ByteCursor::new(entry_bytes);
                    let (width, opcode_value) = match #crate_path::core::decode_instruction_header(&mut cursor) {
                        Ok(result) => result,
                        Err(e) => {
                            errors.push(#crate_path::core::VerificationError::new(
                                ordinal,
                                format!("decode error at instruction {}: {:?}", ordinal, e),
                            ));
                            continue;
                        }
                    };
                    let opcode_id = match OpcodeId::try_from(opcode_value as u16) {
                        Ok(id) => id,
                        Err(_) => {
                            errors.push(#crate_path::core::VerificationError::new(
                                ordinal,
                                format!("invalid opcode value: {}", opcode_value),
                            ));
                            continue;
                        }
                    };
                    match opcode_id {
                        #(#stack_balance_arms)*
                        #(#jump_target_arms)*
                        #(#custom_arms)*
                        _ => {}
                    }
                }
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(errors)
                }
            }
        }
    } else {
        quote! {}
    };

    // Generate link and link_with functions with appropriate return types.
    let (link_fn, link_with_fn) = if has_debug {
        let debug_ident = debug_model.unwrap().name.ident.clone();
        let link_fn = quote! {
            pub fn link<I>(
                instructions: I,
                debug_table: #crate_path::core::DebugTable<#debug_ident>,
            ) -> Result<#crate_path::core::LinkedProgram<#debug_ident>, #crate_path::core::LinkError>
            where
                I: IntoIterator<Item = LinkableInstruction>,
            {
                link_with(instructions, debug_table, #crate_path::core::NativeHeapBackend::new())
            }
        };
        let link_with_fn = quote! {
            pub fn link_with<I, B>(
                instructions: I,
                debug_table: #crate_path::core::DebugTable<#debug_ident>,
                mut backend: B,
            ) -> Result<#crate_path::core::LinkedProgram<#debug_ident>, #crate_path::core::LinkError>
            where
                I: IntoIterator<Item = LinkableInstruction>,
                B: #crate_path::core::LinkBackendV2<Bytecode = #crate_path::core::NativeLinkedBytecode>,
            {
                let instructions = instructions.into_iter().collect::<Vec<_>>();
                let mut widths = Vec::with_capacity(instructions.len());
                let mut metadata_counts = [0u32; #schema_opcode_len];
                let mut metadata_slots = [0u32; #schema_opcode_len];
                let mut code_len = 0usize;

                for instruction in &instructions {
                    match instruction {
                        #(#link_analysis_arms)*
                    }
                }

                let mut label_offsets =
                    std::collections::HashMap::<#crate_path::core::Label, usize>::new();

                loop {
                    label_offsets.clear();
                    let mut offset = 0usize;
                    let mut width_index = 0usize;

                    for instruction in &instructions {
                        match instruction {
                            #(#link_layout_arms)*
                        }
                    }

                    debug_assert_eq!(width_index, widths.len());
                    code_len = offset;

                    let mut changed = false;
                    let mut metadata_slots = [0u32; #schema_opcode_len];
                    let mut offset = 0usize;
                    let mut width_index = 0usize;

                    for instruction in &instructions {
                        match instruction {
                            #(#link_fixpoint_arms)*
                        }
                    }

                    debug_assert_eq!(width_index, widths.len());

                    if !changed {
                        break;
                    }
                }

                let metadata_types: Box<[#crate_path::core::MetadataTypeSpec]> =
                    vec![#(#metadata_type_entries)*].into_boxed_slice();
                let metadata_counts = metadata_counts.into_iter().collect::<Vec<_>>().into_boxed_slice();
                let metadata_layout =
                    #crate_path::core::plan_metadata_table(&metadata_counts, &metadata_types)?;

                let spec = #crate_path::core::LinkSpec {
                    code_size: code_len,
                    instruction_count: widths.len(),
                    metadata_layout: metadata_layout.layout,
                };

                let mut allocations = backend.allocate(&spec)?;
                let code_bytes = unsafe { allocations.code.as_mut_slice() };
                let metadata_bytes = unsafe { allocations.metadata.as_mut_slice() };
                let ordinal_entries = unsafe { allocations.ordinals.as_mut_slice() };

                #crate_path::core::initialize_metadata_table(metadata_bytes, &metadata_layout.headers);

                let mut metadata_slots = [0u32; #schema_opcode_len];
                let mut offset = 0usize;
                let mut width_iter = widths.into_iter();
                let mut ordinal_iter = ordinal_entries.iter_mut();

                let mut debug_ranges: Vec<(std::ops::Range<usize>, #crate_path::core::DebugId)> = Vec::new();

                for instruction in instructions.into_iter() {
                    match instruction {
                        LinkableInstruction::__Bind(_) => continue,
                        instruction => {
                            let Some(width) = width_iter.next() else {
                                unreachable!("every materialized instruction must have a width")
                            };
                            let Some(entry) = ordinal_iter.next() else {
                                unreachable!("every materialized instruction must have an ordinal entry")
                            };
                            match instruction {
                                #(#link_materialize_arms)*
                            }
                        }
                    }
                }

                debug_assert!(width_iter.next().is_none());
                debug_assert!(ordinal_iter.next().is_none());
                debug_assert_eq!(offset, code_len);

                let bytecode = backend.commit(
                    allocations,
                    #crate_path::core::LinkedArtifactSpec {
                        metadata_types,
                        metadata_counts,
                        metadata_layout: metadata_layout.layout,
                    },
                )?;

                let debug_info = if !debug_ranges.is_empty() {
                    Some(#crate_path::core::LinkedDebugInfo::new(
                        debug_table,
                        #crate_path::core::DebugMap::from_ranges(debug_ranges),
                    ))
                } else {
                    None
                };

                Ok(#crate_path::core::LinkedProgram { bytecode, debug_info })
            }
        };
        (link_fn, link_with_fn)
    } else {
        let link_fn = quote! {
            pub fn link<I>(instructions: I) -> Result<LinkedBytecode, #crate_path::core::LinkError>
            where
                I: IntoIterator<Item = LinkableInstruction>,
            {
                link_with(instructions, #crate_path::core::NativeHeapBackend::new())
            }
        };
        let link_with_fn = quote! {
            pub fn link_with<I, B>(
                instructions: I,
                mut backend: B,
            ) -> Result<B::Bytecode, #crate_path::core::LinkError>
            where
                I: IntoIterator<Item = LinkableInstruction>,
                B: #crate_path::core::LinkBackendV2,
            {
                let instructions = instructions.into_iter().collect::<Vec<_>>();
                let mut widths = Vec::with_capacity(instructions.len());
                let mut metadata_counts = [0u32; #schema_opcode_len];
                let mut metadata_slots = [0u32; #schema_opcode_len];
                let mut code_len = 0usize;

                for instruction in &instructions {
                    match instruction {
                        #(#link_analysis_arms)*
                    }
                }

                let mut label_offsets =
                    std::collections::HashMap::<#crate_path::core::Label, usize>::new();

                loop {
                    label_offsets.clear();
                    let mut offset = 0usize;
                    let mut width_index = 0usize;

                    for instruction in &instructions {
                        match instruction {
                            #(#link_layout_arms)*
                        }
                    }

                    debug_assert_eq!(width_index, widths.len());
                    code_len = offset;

                    let mut changed = false;
                    let mut metadata_slots = [0u32; #schema_opcode_len];
                    let mut offset = 0usize;
                    let mut width_index = 0usize;

                    for instruction in &instructions {
                        match instruction {
                            #(#link_fixpoint_arms)*
                        }
                    }

                    debug_assert_eq!(width_index, widths.len());

                    if !changed {
                        break;
                    }
                }

                let metadata_types: Box<[#crate_path::core::MetadataTypeSpec]> =
                    vec![#(#metadata_type_entries)*].into_boxed_slice();
                let metadata_counts = metadata_counts.into_iter().collect::<Vec<_>>().into_boxed_slice();
                let metadata_layout =
                    #crate_path::core::plan_metadata_table(&metadata_counts, &metadata_types)?;

                let spec = #crate_path::core::LinkSpec {
                    code_size: code_len,
                    instruction_count: widths.len(),
                    metadata_layout: metadata_layout.layout,
                };

                let mut allocations = backend.allocate(&spec)?;
                let code_bytes = unsafe { allocations.code.as_mut_slice() };
                let metadata_bytes = unsafe { allocations.metadata.as_mut_slice() };
                let ordinal_entries = unsafe { allocations.ordinals.as_mut_slice() };

                #crate_path::core::initialize_metadata_table(metadata_bytes, &metadata_layout.headers);

                let mut metadata_slots = [0u32; #schema_opcode_len];
                let mut offset = 0usize;
                let mut width_iter = widths.into_iter();
                let mut ordinal_iter = ordinal_entries.iter_mut();

                for instruction in instructions.into_iter() {
                    match instruction {
                        LinkableInstruction::__Bind(_) => continue,
                        instruction => {
                            let Some(width) = width_iter.next() else {
                                unreachable!("every materialized instruction must have a width")
                            };
                            let Some(entry) = ordinal_iter.next() else {
                                unreachable!("every materialized instruction must have an ordinal entry")
                            };
                            match instruction {
                                #(#link_materialize_arms)*
                            }
                        }
                    }
                }

                debug_assert!(width_iter.next().is_none());
                debug_assert!(ordinal_iter.next().is_none());
                debug_assert_eq!(offset, code_len);

                backend.commit(
                    allocations,
                    #crate_path::core::LinkedArtifactSpec {
                        metadata_types,
                        metadata_counts,
                        metadata_layout: metadata_layout.layout,
                    },
                )
            }
        };
        (link_fn, link_with_fn)
    };

    quote! {
        #[allow(non_snake_case, non_upper_case_globals)]
        pub mod generated {
            use super::*;

            #debug_struct

            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
            #[repr(u16)]
            pub enum SectionId {
                #(#section_variants)*
            }

            impl SectionId {
                pub const fn name(self) -> &'static str {
                    match self {
                        #(#section_name_matches)*
                    }
                }

                pub const fn kebab_name(self) -> &'static str {
                    match self {
                        #(#section_kebab_name_matches)*
                    }
                }

                pub fn opcodes(self) -> &'static [OpcodeId] {
                    match self {
                        #(#section_opcode_matches)*
                    }
                }

                pub fn source_items(self) -> &'static [SectionItem] {
                    match self {
                        #(#section_source_item_matches)*
                    }
                }
            }

            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
            #[repr(u16)]
            pub enum OpcodeId {
                #(#opcode_variants)*
            }

            impl OpcodeId {
                pub const fn name(self) -> &'static str {
                    match self {
                        #(#opcode_name_matches)*
                    }
                }

                pub const fn kebab_name(self) -> &'static str {
                    match self {
                        #(#opcode_kebab_name_matches)*
                    }
                }

                pub const fn section(self) -> SectionId {
                    match self {
                        #(#opcode_section_matches)*
                    }
                }

                pub fn descriptor(self) -> &'static OpcodeDescriptor {
                    match self {
                        #(#opcode_descriptor_matches)*
                    }
                }
            }

            impl TryFrom<u16> for OpcodeId {
                type Error = ();

                fn try_from(value: u16) -> Result<Self, Self::Error> {
                    match value {
                        #(
                            x if x == OpcodeId::#opcode_id_idents as u16 => Ok(OpcodeId::#opcode_id_idents),
                        )*
                        _ => Err(()),
                    }
                }
            }

            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
            pub struct OpcodeDescriptor {
                pub id: OpcodeId,
                pub section: SectionId,
                pub name: &'static str,
                pub mnemonic: &'static str,
                pub operand_count: usize,
                pub has_metadata: bool,
                pub has_relocations: bool,
            }

            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
            pub enum SectionItem {
                Opcode(OpcodeId),
                Group {
                    name: &'static str,
                    members: &'static [OpcodeId],
                },
            }

            #(#opcode_items)*
            #(#group_items)*
            #(#section_items)*

            pub trait InstructionVisitor {
                type Output;

                #(#visitor_methods)*
            }

            pub enum Instruction {
                #(#instruction_variants)*
            }

            pub enum LinkableInstruction {
                #(#linkable_variants)*
            }

            #with_debug_impl

            pub type LinkedBytecode = #crate_path::core::NativeLinkedBytecode;

            pub fn bind(label: #crate_path::core::Label) -> LinkableInstruction {
                LinkableInstruction::__Bind(label)
            }

            pub struct DecodedInstruction {
                pub instruction: Instruction,
                pub width: #crate_path::core::OperandWidth,
                pub offset: usize,
                pub size: usize,
                pub raw_bytes: Box<[u8]>,
            }

            #packed_schema_items

            impl Instruction {
                pub fn decode(input: &[u8]) -> Result<Self, #crate_path::core::DecodeError> {
                    let mut cursor = #crate_path::core::ByteCursor::new(input);
                    Self::decode_from(&mut cursor)
                }

                pub fn decode_record(
                    input: &[u8],
                ) -> Result<DecodedInstruction, #crate_path::core::DecodeError> {
                    let mut cursor = #crate_path::core::ByteCursor::new(input);
                    Self::decode_record_from(&mut cursor)
                }

                pub fn decode_from(
                    input: &mut #crate_path::core::ByteCursor<'_>,
                ) -> Result<Self, #crate_path::core::DecodeError> {
                    Ok(Self::decode_record_from(input)?.instruction)
                }

                pub fn decode_record_from(
                    input: &mut #crate_path::core::ByteCursor<'_>,
                ) -> Result<DecodedInstruction, #crate_path::core::DecodeError> {
                    let offset = input.position();
                    let (width, opcode) = #crate_path::core::decode_instruction_header(input)?;
                    let instruction = match opcode {
                        #(#instruction_decode_arms)*
                        _ => Err(#crate_path::core::DecodeError::InvalidOpcode(opcode)),
                    }?;
                    let end = input.position();
                    let size = end - offset;
                    let raw_bytes = input.slice(offset..end)?.to_vec().into_boxed_slice();
                    Ok(DecodedInstruction {
                        instruction,
                        width,
                        offset,
                        size,
                        raw_bytes,
                    })
                }

                pub const fn opcode_id(&self) -> OpcodeId {
                    match self {
                        #(#instruction_id_arms)*
                    }
                }

                pub fn accept<V: InstructionVisitor>(&self, visitor: &mut V) -> V::Output {
                    match self {
                        #(#accept_arms)*
                    }
                }

                pub fn disassemble_with<Ctx, Formatter>(
                    &self,
                    context: &Ctx,
                    formatter: &Formatter,
                    instruction: #crate_path::core::InstructionFormatSpec<'_>,
                    options: &#crate_path::core::DisassemblerOptions,
                ) -> Result<String, #crate_path::core::DisassembleError>
                where
                    Self: #crate_path::core::Disassemble<Ctx, Formatter>,
                {
                    <Self as #crate_path::core::Disassemble<Ctx, Formatter>>::disassemble(
                        self,
                        context,
                        formatter,
                        instruction,
                        options,
                    )
                    .map_err(#crate_path::core::DisassembleError::from)
                }
            }

            impl DecodedInstruction {
                fn format_spec_with_bytes_width(
                    &self,
                    ordinal: usize,
                    bytes_column_width: usize,
                ) -> #crate_path::core::InstructionFormatSpec<'_> {
                    #crate_path::core::InstructionFormatSpec {
                        ordinal,
                        offset: self.offset,
                        size: self.size,
                        width: self.width,
                        opcode_name: self.instruction.opcode_id().name(),
                        raw_bytes: &self.raw_bytes,
                        raw_words: None,
                        bytes_column_width,
                    }
                }

                pub fn format_spec(
                    &self,
                    ordinal: usize,
                ) -> #crate_path::core::InstructionFormatSpec<'_> {
                    let bytes_column_width = #crate_path::core::rendered_bytes_width(&self.raw_bytes);
                    self.format_spec_with_bytes_width(ordinal, bytes_column_width)
                }

                fn disassemble_into_with_bytes_width<Ctx, Formatter>(
                    &self,
                    output: &mut dyn std::fmt::Write,
                    ordinal: usize,
                    bytes_column_width: usize,
                    context: &Ctx,
                    formatter: &Formatter,
                    options: &#crate_path::core::DisassemblerOptions,
                ) -> Result<(), #crate_path::core::DisassembleError>
                where
                    Instruction: #crate_path::core::Disassemble<Ctx, Formatter>,
                {
                    let instruction = self.format_spec_with_bytes_width(ordinal, bytes_column_width);
                    <Instruction as #crate_path::core::Disassemble<Ctx, Formatter>>::disassemble_into(
                        &self.instruction,
                        output,
                        context,
                        formatter,
                        instruction,
                        options,
                    )
                    .map_err(#crate_path::core::DisassembleError::from)
                }

                pub fn disassemble_into<Ctx, Formatter>(
                    &self,
                    output: &mut dyn std::fmt::Write,
                    ordinal: usize,
                    context: &Ctx,
                    formatter: &Formatter,
                    options: &#crate_path::core::DisassemblerOptions,
                ) -> Result<(), #crate_path::core::DisassembleError>
                where
                    Instruction: #crate_path::core::Disassemble<Ctx, Formatter>,
                {
                    let bytes_column_width = if options.show_bytes {
                        #crate_path::core::rendered_bytes_width(&self.raw_bytes)
                    } else {
                        0
                    };
                    self.disassemble_into_with_bytes_width(
                        output,
                        ordinal,
                        bytes_column_width,
                        context,
                        formatter,
                        options,
                    )
                }

                pub fn disassemble_with<Ctx, Formatter>(
                    &self,
                    ordinal: usize,
                    context: &Ctx,
                    formatter: &Formatter,
                    options: &#crate_path::core::DisassemblerOptions,
                ) -> Result<String, #crate_path::core::DisassembleError>
                where
                    Instruction: #crate_path::core::Disassemble<Ctx, Formatter>,
                {
                    let mut output = String::new();
                    self.disassemble_into(&mut output, ordinal, context, formatter, options)?;
                    Ok(output)
                }
            }

            impl #crate_path::core::Encode for Instruction {
                fn encoded_len(&self, width: #crate_path::core::OperandWidth) -> usize {
                    match self {
                        #(#instruction_encode_len_arms)*
                    }
                }

                fn encode_with_width<S: #crate_path::core::ByteSink + ?Sized>(
                    &self,
                    output: &mut S,
                    width: #crate_path::core::OperandWidth,
                ) {
                    match self {
                        #(#instruction_encode_arms)*
                    }
                }
            }

            impl #crate_path::core::Emit for Instruction {
                fn check_width(&self, width: #crate_path::core::OperandWidth) -> bool {
                    match self {
                        #(#instruction_check_arms)*
                    }
                }

                fn emit_with_width<S: #crate_path::core::ByteSink + ?Sized>(
                    &self,
                    output: &mut S,
                    width: #crate_path::core::OperandWidth,
                ) -> Result<(), #crate_path::core::EmitError> {
                    match self {
                        #(#instruction_emit_arms)*
                    }
                }

                fn smallest_width(&self) -> Result<#crate_path::core::OperandWidth, #crate_path::core::EmitError> {
                    match self {
                        #(#instruction_smallest_width_arms)*
                    }
                }
            }

            impl<Ctx, Formatter> #crate_path::core::Disassemble<Ctx, Formatter> for Instruction
            where
                #(#instruction_disassemble_bounds)*
            {
                fn disassemble_into(
                    &self,
                    output: &mut dyn std::fmt::Write,
                    context: &Ctx,
                    formatter: &Formatter,
                    instruction: #crate_path::core::InstructionFormatSpec<'_>,
                    options: &#crate_path::core::DisassemblerOptions,
                ) -> std::fmt::Result {
                    match self {
                        #(#instruction_disassemble_arms)*
                    }
                }
            }

            #link_fn

            #link_with_fn

            pub fn decode_linked_record<V: #crate_path::core::LinkedBytecodeView + ?Sized>(
                linked: &V,
                ordinal: usize,
            ) -> Result<DecodedInstruction, #crate_path::core::DecodeError> {
                let entries = linked.instructions();
                let entry = entries
                    .get(ordinal)
                    .ok_or(#crate_path::core::DecodeError::InvalidInstructionOrdinal(ordinal))?;
                let start = entry.offset as usize;
                let end = entries
                    .get(ordinal + 1)
                    .map(|next| next.offset as usize)
                    .unwrap_or_else(|| linked.bytes().len());
                let bytes = linked
                    .bytes()
                    .get(start..end)
                    .ok_or(#crate_path::core::DecodeError::UnexpectedEof)?;
                let mut cursor = #crate_path::core::ByteCursor::new(bytes);
                let mut decoded = Instruction::decode_record_from(&mut cursor)?;
                decoded.offset = start;
                decoded.size = end - start;
                Ok(decoded)
            }

            pub fn disassemble_bytes_with<Ctx, Formatter>(
                bytes: &[u8],
                context: &Ctx,
                formatter: &Formatter,
                options: &#crate_path::core::DisassemblerOptions,
            ) -> Result<String, #crate_path::core::DisassembleError>
            where
                Instruction: #crate_path::core::Disassemble<Ctx, Formatter>,
            {
                let mut cursor = #crate_path::core::ByteCursor::new(bytes);
                let mut decoded = Vec::new();

                while cursor.remaining() > 0 {
                    decoded.push(Instruction::decode_record_from(&mut cursor)?);
                }

                let bytes_column_width = if options.show_bytes {
                    decoded
                        .iter()
                        .map(|instruction| {
                            #crate_path::core::rendered_bytes_width(&instruction.raw_bytes)
                        })
                        .max()
                        .unwrap_or(0)
                } else {
                    0
                };

                let mut output = String::new();
                for (ordinal, instruction) in decoded.iter().enumerate() {
                    if ordinal > 0 {
                        output.push('\n');
                    }

                    instruction.disassemble_into_with_bytes_width(
                        &mut output,
                        ordinal,
                        bytes_column_width,
                        context,
                        formatter,
                        options,
                    )?;
                }

                Ok(output)
            }

            pub fn disassemble_bytes<Ctx>(
                bytes: &[u8],
                context: &Ctx,
                options: &#crate_path::core::DisassemblerOptions,
            ) -> Result<String, #crate_path::core::DisassembleError>
            where
                Instruction: #crate_path::core::Disassemble<
                    Ctx,
                    #crate_path::core::DefaultOperandFormatter,
                >,
            {
                let formatter = #crate_path::core::DefaultOperandFormatter;
                disassemble_bytes_with(bytes, context, &formatter, options)
            }

            pub fn disassemble_linked_with<Ctx, Formatter, V>(
                linked: &V,
                context: &Ctx,
                formatter: &Formatter,
                options: &#crate_path::core::DisassemblerOptions,
            ) -> Result<String, #crate_path::core::DisassembleError>
            where
                V: #crate_path::core::LinkedBytecodeView + ?Sized,
                Instruction: #crate_path::core::Disassemble<Ctx, Formatter>,
            {
                let mut decoded = Vec::with_capacity(linked.instruction_count());
                for ordinal in 0..linked.instruction_count() {
                    decoded.push(decode_linked_record(linked, ordinal)?);
                }

                let bytes_column_width = if options.show_bytes {
                    decoded
                        .iter()
                        .map(|instruction| {
                            #crate_path::core::rendered_bytes_width(&instruction.raw_bytes)
                        })
                        .max()
                        .unwrap_or(0)
                } else {
                    0
                };

                let mut output = String::new();
                for (ordinal, instruction) in decoded.iter().enumerate() {
                    if ordinal > 0 {
                        output.push('\n');
                    }

                    instruction.disassemble_into_with_bytes_width(
                        &mut output,
                        ordinal,
                        bytes_column_width,
                        context,
                        formatter,
                        options,
                    )?;
                }

                Ok(output)
            }

            pub fn disassemble_linked<Ctx, V>(
                linked: &V,
                context: &Ctx,
                options: &#crate_path::core::DisassemblerOptions,
            ) -> Result<String, #crate_path::core::DisassembleError>
            where
                V: #crate_path::core::LinkedBytecodeView + ?Sized,
                Instruction: #crate_path::core::Disassemble<
                    Ctx,
                    #crate_path::core::DefaultOperandFormatter,
                >,
            {
                let formatter = #crate_path::core::DefaultOperandFormatter;
                disassemble_linked_with(linked, context, &formatter, options)
            }

            #interpreter_macro_items

            #verify_code
        }

        pub use generated::*;
    }
}
