use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::Ident;

use crate::model::{OpcodeModel, SchemaModel};

#[derive(Clone, Copy)]
pub(crate) enum InterpreterMacroFlavor {
    Expression,
    TailPath,
}

pub(crate) fn generate_interpreter_macro_items(
    schema: &SchemaModel,
    crate_path: &TokenStream2,
    macro_suffix: &str,
) -> TokenStream2 {
    let define_macro = generate_interpreter_macro(
        schema,
        crate_path,
        &format_ident!("__bytecode_define_interpreter_{macro_suffix}"),
        InterpreterMacroFlavor::Expression,
    );
    let tail_macro = generate_interpreter_macro(
        schema,
        crate_path,
        &format_ident!("__bytecode_define_tail_interpreter_{macro_suffix}"),
        InterpreterMacroFlavor::TailPath,
    );

    let define_hidden = format_ident!("__bytecode_define_interpreter_{macro_suffix}");
    let tail_hidden = format_ident!("__bytecode_define_tail_interpreter_{macro_suffix}");
    let define_docs =
        crate::doc_gen::render_doc_attrs(&crate::doc_gen::define_interpreter_doc_lines());
    let tail_docs =
        crate::doc_gen::render_doc_attrs(&crate::doc_gen::define_tail_interpreter_doc_lines());

    quote! {
        #define_macro
        #tail_macro

        #define_docs
        pub use #define_hidden as define_interpreter;
        #tail_docs
        pub use #tail_hidden as define_tail_interpreter;
    }
}

pub(crate) fn generate_interpreter_macro(
    schema: &SchemaModel,
    crate_path: &TokenStream2,
    hidden_macro_ident: &Ident,
    flavor: InterpreterMacroFlavor,
) -> TokenStream2 {
    let rules = generate_interpreter_rules(schema, crate_path, flavor);
    let fallback_message = match flavor {
        InterpreterMacroFlavor::Expression => {
            "invalid generated::define_interpreter! input: expected a free function signature, an ip hook block, an invalid arm, and one opcode arm per schema opcode in schema order"
        }
        InterpreterMacroFlavor::TailPath => {
            "invalid generated::define_tail_interpreter! input: expected a free function signature, an ip hook block, an invalid arm, and one opcode arm per schema opcode in schema order"
        }
    };

    quote! {
        #[macro_export]
        macro_rules! #hidden_macro_ident {
            #(#rules)*
            ($($tt:tt)*) => {
                compile_error!(#fallback_message);
            };
        }
    }
}

pub(crate) fn generate_interpreter_rules(
    schema: &SchemaModel,
    crate_path: &TokenStream2,
    flavor: InterpreterMacroFlavor,
) -> Vec<TokenStream2> {
    let meta_var = format_ident!("__meta");
    let vis_var = format_ident!("__vis");
    let name_var = format_ident!("__name");
    let param_var = format_ident!("__param");
    let param_ty_var = format_ident!("__param_ty");
    let abi_var = format_ident!("__abi");
    let ret_var = format_ident!("__ret");
    let load_var = format_ident!("__load");
    let next_ip_binding_var = format_ident!("__next_ip_binding");
    let store_var = format_ident!("__store");

    let invalid_opcode_binding_var = format_ident!("__invalid_opcode_binding");
    let invalid_width_binding_var = format_ident!("__invalid_width_binding");
    let invalid_expr_var = format_ident!("__invalid_expr");
    let unit_runtime = generate_interpreter_runtime(schema, crate_path, flavor, &quote!(()));
    let return_runtime =
        generate_interpreter_runtime(schema, crate_path, flavor, &quote!($#ret_var));

    let arms_pattern = generate_interpreter_arm_pattern(
        schema,
        &invalid_opcode_binding_var,
        &invalid_width_binding_var,
        &invalid_expr_var,
    );

    let safe_no_return = quote! {
        (
            $(#[$#meta_var:meta])*
            $#vis_var:vis fn $#name_var:ident($( $#param_var:ident : $#param_ty_var:ty ),* $(,)?);
            ip {
                load = $#load_var:block;
                store($#next_ip_binding_var:ident) = $#store_var:block;
            }
            #arms_pattern
        ) => {
            #[allow(unused_variables)]
            $(#[$#meta_var])*
            $#vis_var fn $#name_var($( $#param_var : $#param_ty_var ),*) {
                #unit_runtime
            }
        };
    };

    let safe_with_return = quote! {
        (
            $(#[$#meta_var:meta])*
            $#vis_var:vis fn $#name_var:ident($( $#param_var:ident : $#param_ty_var:ty ),* $(,)?)
                -> $#ret_var:ty;
            ip {
                load = $#load_var:block;
                store($#next_ip_binding_var:ident) = $#store_var:block;
            }
            #arms_pattern
        ) => {
            #[allow(unused_variables)]
            $(#[$#meta_var])*
            $#vis_var fn $#name_var($( $#param_var : $#param_ty_var ),*) -> $#ret_var {
                #return_runtime
            }
        };
    };

    let unsafe_no_return = quote! {
        (
            $(#[$#meta_var:meta])*
            $#vis_var:vis unsafe fn $#name_var:ident($( $#param_var:ident : $#param_ty_var:ty ),* $(,)?);
            ip {
                load = $#load_var:block;
                store($#next_ip_binding_var:ident) = $#store_var:block;
            }
            #arms_pattern
        ) => {
            #[allow(unused_variables)]
            $(#[$#meta_var])*
            $#vis_var unsafe fn $#name_var($( $#param_var : $#param_ty_var ),*) {
                #unit_runtime
            }
        };
    };

    let unsafe_with_return = quote! {
        (
            $(#[$#meta_var:meta])*
            $#vis_var:vis unsafe fn $#name_var:ident($( $#param_var:ident : $#param_ty_var:ty ),* $(,)?)
                -> $#ret_var:ty;
            ip {
                load = $#load_var:block;
                store($#next_ip_binding_var:ident) = $#store_var:block;
            }
            #arms_pattern
        ) => {
            #[allow(unused_variables)]
            $(#[$#meta_var])*
            $#vis_var unsafe fn $#name_var($( $#param_var : $#param_ty_var ),*) -> $#ret_var {
                #return_runtime
            }
        };
    };

    let extern_no_return = quote! {
        (
            $(#[$#meta_var:meta])*
            $#vis_var:vis extern $#abi_var:literal fn $#name_var:ident($( $#param_var:ident : $#param_ty_var:ty ),* $(,)?);
            ip {
                load = $#load_var:block;
                store($#next_ip_binding_var:ident) = $#store_var:block;
            }
            #arms_pattern
        ) => {
            #[allow(unused_variables)]
            $(#[$#meta_var])*
            $#vis_var extern $#abi_var fn $#name_var($( $#param_var : $#param_ty_var ),*) {
                #unit_runtime
            }
        };
    };

    let extern_with_return = quote! {
        (
            $(#[$#meta_var:meta])*
            $#vis_var:vis extern $#abi_var:literal fn $#name_var:ident($( $#param_var:ident : $#param_ty_var:ty ),* $(,)?)
                -> $#ret_var:ty;
            ip {
                load = $#load_var:block;
                store($#next_ip_binding_var:ident) = $#store_var:block;
            }
            #arms_pattern
        ) => {
            #[allow(unused_variables)]
            $(#[$#meta_var])*
            $#vis_var extern $#abi_var fn $#name_var($( $#param_var : $#param_ty_var ),*) -> $#ret_var {
                #return_runtime
            }
        };
    };

    let unsafe_extern_no_return = quote! {
        (
            $(#[$#meta_var:meta])*
            $#vis_var:vis unsafe extern $#abi_var:literal fn $#name_var:ident($( $#param_var:ident : $#param_ty_var:ty ),* $(,)?);
            ip {
                load = $#load_var:block;
                store($#next_ip_binding_var:ident) = $#store_var:block;
            }
            #arms_pattern
        ) => {
            #[allow(unused_variables)]
            $(#[$#meta_var])*
            $#vis_var unsafe extern $#abi_var fn $#name_var($( $#param_var : $#param_ty_var ),*) {
                #unit_runtime
            }
        };
    };

    let unsafe_extern_with_return = quote! {
        (
            $(#[$#meta_var:meta])*
            $#vis_var:vis unsafe extern $#abi_var:literal fn $#name_var:ident($( $#param_var:ident : $#param_ty_var:ty ),* $(,)?)
                -> $#ret_var:ty;
            ip {
                load = $#load_var:block;
                store($#next_ip_binding_var:ident) = $#store_var:block;
            }
            #arms_pattern
        ) => {
            #[allow(unused_variables)]
            $(#[$#meta_var])*
            $#vis_var unsafe extern $#abi_var fn $#name_var($( $#param_var : $#param_ty_var ),*) -> $#ret_var {
                #return_runtime
            }
        };
    };

    vec![
        safe_no_return,
        safe_with_return,
        unsafe_no_return,
        unsafe_with_return,
        extern_no_return,
        extern_with_return,
        unsafe_extern_no_return,
        unsafe_extern_with_return,
    ]
}

pub(crate) fn generate_interpreter_arm_pattern(
    schema: &SchemaModel,
    invalid_opcode_binding_var: &Ident,
    invalid_width_binding_var: &Ident,
    invalid_expr_var: &Ident,
) -> TokenStream2 {
    let opcode_patterns = schema.opcodes.iter().map(|opcode| {
        let opcode_ident = &opcode.name.ident;
        let binding_ident = format_ident!("__{}_binding", opcode.name.snake_case);
        let handler_ident = format_ident!("__{}_handler", opcode.name.snake_case);
        quote!(#opcode_ident($#binding_ident:ident) => $#handler_ident:expr,)
    });

    quote! {
        invalid($#invalid_opcode_binding_var:ident, $#invalid_width_binding_var:ident) => $#invalid_expr_var:expr,
        #(#opcode_patterns)*
        $(,)?
    }
}

pub(crate) fn generate_interpreter_runtime(
    schema: &SchemaModel,
    crate_path: &TokenStream2,
    flavor: InterpreterMacroFlavor,
    ret_ty: &TokenStream2,
) -> TokenStream2 {
    match flavor {
        InterpreterMacroFlavor::Expression => {
            generate_expression_interpreter_runtime(schema, crate_path, ret_ty)
        }
        InterpreterMacroFlavor::TailPath => generate_tail_interpreter_runtime(schema, crate_path),
    }
}

pub(crate) fn generate_expression_interpreter_runtime(
    schema: &SchemaModel,
    crate_path: &TokenStream2,
    ret_ty: &TokenStream2,
) -> TokenStream2 {
    let param_var = format_ident!("__param");
    let param_ty_var = format_ident!("__param_ty");
    let load_var = format_ident!("__load");
    let local_invalid_raw_opcode_ident = format_ident!("__invalid_raw_opcode");
    let local_invalid_opcode_ident = format_ident!("__invalid_opcode");
    let local_invalid_width_ident = format_ident!("__invalid_width");
    let invalid_opcode_binding_var = format_ident!("__invalid_opcode_binding");
    let invalid_width_binding_var = format_ident!("__invalid_width_binding");
    let invalid_expr_var = format_ident!("__invalid_expr");
    let invalid_action = quote! {
        let $#invalid_opcode_binding_var = #local_invalid_opcode_ident;
        let $#invalid_width_binding_var = #local_invalid_width_ident;
        return $#invalid_expr_var;
    };

    let per_opcode_decoders = schema.opcodes.iter().enumerate().map(|(index, opcode)| {
        generate_expression_interpreter_decoder(opcode, index as u32, crate_path, ret_ty)
    });

    let narrow_assignments = schema
        .opcodes
        .iter()
        .enumerate()
        .filter(|(index, _)| *index <= (u8::MAX as usize - 2));

    let narrow_table_assignments = narrow_assignments.clone().map(|(index, opcode)| {
        let decoder_ident = format_ident!("decode_{}_narrow", opcode.name.snake_case);
        quote! {
            table[#index] = #decoder_ident as DecoderFn;
        }
    });

    let wide16_table_assignments = schema.opcodes.iter().enumerate().map(|(index, opcode)| {
        let decoder_ident = format_ident!("decode_{}_wide16", opcode.name.snake_case);
        quote! {
            table[#index] = #decoder_ident as DecoderFn;
        }
    });

    let wide32_table_assignments = schema.opcodes.iter().enumerate().map(|(index, opcode)| {
        let decoder_ident = format_ident!("decode_{}_wide32", opcode.name.snake_case);
        quote! {
            table[#index] = #decoder_ident as DecoderFn;
        }
    });

    quote! {
        type DecoderFn = fn($( $#param_ty_var, )* *const u8) -> #ret_ty;

        #[allow(unused_variables)]
        #[inline(always)]
        fn invalid_narrow($( $#param_var: $#param_ty_var, )* __ip: *const u8) -> #ret_ty {
            let (#local_invalid_raw_opcode_ident, _) = unsafe {
                <u8 as #crate_path::core::Operand>::decode_from_ptr(
                    #crate_path::core::OperandWidth::Narrow,
                    __ip.sub(1),
                )
            };
            let #local_invalid_opcode_ident = u32::from(#local_invalid_raw_opcode_ident);
            let #local_invalid_width_ident = #crate_path::core::OperandWidth::Narrow;
            #invalid_action
        }

        #[allow(unused_variables)]
        #[inline(always)]
        fn invalid_wide16($( $#param_var: $#param_ty_var, )* __ip: *const u8) -> #ret_ty {
            let (#local_invalid_raw_opcode_ident, _) = unsafe {
                <u16 as #crate_path::core::Operand>::decode_from_ptr(
                    #crate_path::core::OperandWidth::Wide16,
                    __ip.sub(2),
                )
            };
            let #local_invalid_opcode_ident = u32::from(#local_invalid_raw_opcode_ident);
            let #local_invalid_width_ident = #crate_path::core::OperandWidth::Wide16;
            #invalid_action
        }

        #[allow(unused_variables)]
        #[inline(always)]
        fn invalid_wide32($( $#param_var: $#param_ty_var, )* __ip: *const u8) -> #ret_ty {
            let (#local_invalid_raw_opcode_ident, _) = unsafe {
                <u32 as #crate_path::core::Operand>::decode_from_ptr(
                    #crate_path::core::OperandWidth::Wide32,
                    __ip.sub(4),
                )
            };
            let #local_invalid_opcode_ident = #local_invalid_raw_opcode_ident;
            let #local_invalid_width_ident = #crate_path::core::OperandWidth::Wide32;
            #invalid_action
        }

        #[allow(unused_variables)]
        #[inline(always)]
        fn handle_wide16($( $#param_var: $#param_ty_var, )* __ip: *const u8) -> #ret_ty {
            let (__opcode, __next_ip) = unsafe {
                <u16 as #crate_path::core::Operand>::decode_from_ptr(
                    #crate_path::core::OperandWidth::Wide16,
                    __ip,
                )
            };
            let decoder = INSTRUCTIONS16[usize::from(__opcode)];
            decoder($( $#param_var, )* __next_ip)
        }

        #[allow(unused_variables)]
        #[inline(always)]
        fn handle_wide32($( $#param_var: $#param_ty_var, )* __ip: *const u8) -> #ret_ty {
            let (__opcode, __next_ip) = unsafe {
                <u32 as #crate_path::core::Operand>::decode_from_ptr(
                    #crate_path::core::OperandWidth::Wide32,
                    __ip,
                )
            };
            if __opcode > u16::MAX as u32 {
                let #local_invalid_opcode_ident = __opcode;
                let #local_invalid_width_ident = #crate_path::core::OperandWidth::Wide32;
                #invalid_action
            }
            let decoder = INSTRUCTIONS32[__opcode as usize];
            decoder($( $#param_var, )* __next_ip)
        }

        #(#per_opcode_decoders)*

        const fn build_instructions8() -> [DecoderFn; 256] {
            let mut table = [invalid_narrow as DecoderFn; 256];
            #(#narrow_table_assignments)*
            table[0xFE] = handle_wide16 as DecoderFn;
            table[0xFF] = handle_wide32 as DecoderFn;
            table
        }

        const fn build_instructions16() -> [DecoderFn; 65536] {
            let mut table = [invalid_wide16 as DecoderFn; 65536];
            #(#wide16_table_assignments)*
            table
        }

        const fn build_instructions32() -> [DecoderFn; 65536] {
            let mut table = [invalid_wide32 as DecoderFn; 65536];
            #(#wide32_table_assignments)*
            table
        }

        static INSTRUCTIONS8: [DecoderFn; 256] = build_instructions8();
        static INSTRUCTIONS16: [DecoderFn; 65536] = build_instructions16();
        static INSTRUCTIONS32: [DecoderFn; 65536] = build_instructions32();

        let __instruction_ptr = ($#load_var) as *const u8;
        let (__opcode_prefix, __next_ip) = unsafe {
            <u8 as #crate_path::core::Operand>::decode_from_ptr(
                #crate_path::core::OperandWidth::Narrow,
                __instruction_ptr,
            )
        };
        let decoder = INSTRUCTIONS8[usize::from(__opcode_prefix)];
        decoder($( $#param_var, )* __next_ip)
    }
}

pub(crate) fn generate_tail_interpreter_runtime(
    schema: &SchemaModel,
    crate_path: &TokenStream2,
) -> TokenStream2 {
    let load_var = format_ident!("__load");
    let invalid_opcode_binding_var = format_ident!("__invalid_opcode_binding");
    let invalid_width_binding_var = format_ident!("__invalid_width_binding");
    let invalid_expr_var = format_ident!("__invalid_expr");

    let narrow_opcode_arms = schema
        .opcodes
        .iter()
        .enumerate()
        .filter(|(index, _)| *index <= (u8::MAX as usize - 2))
        .map(|(index, opcode)| {
            generate_tail_interpreter_inline_arm(opcode, index as u32, crate_path)
        })
        .collect::<Vec<_>>();
    let wide_opcode_arms = schema
        .opcodes
        .iter()
        .enumerate()
        .map(|(index, opcode)| {
            generate_tail_interpreter_inline_arm(opcode, index as u32, crate_path)
        })
        .collect::<Vec<_>>();

    let narrow_invalid = generate_tail_interpreter_invalid_expr(
        &invalid_opcode_binding_var,
        &invalid_width_binding_var,
        &invalid_expr_var,
        quote!(u32::from(__opcode_prefix)),
        quote!(#crate_path::core::OperandWidth::Narrow),
    );
    let wide16_invalid = generate_tail_interpreter_invalid_expr(
        &invalid_opcode_binding_var,
        &invalid_width_binding_var,
        &invalid_expr_var,
        quote!(u32::from(__opcode)),
        quote!(#crate_path::core::OperandWidth::Wide16),
    );
    let wide32_invalid = generate_tail_interpreter_invalid_expr(
        &invalid_opcode_binding_var,
        &invalid_width_binding_var,
        &invalid_expr_var,
        quote!(__opcode),
        quote!(#crate_path::core::OperandWidth::Wide32),
    );

    quote! {
        let __instruction_ptr = ($#load_var) as *const u8;
        let (__opcode_prefix, __next_ip) = unsafe {
            <u8 as #crate_path::core::Operand>::decode_from_ptr(
                #crate_path::core::OperandWidth::Narrow,
                __instruction_ptr,
            )
        };

        match __opcode_prefix {
            0xFE => {
                let (__opcode, __ip) = unsafe {
                    <u16 as #crate_path::core::Operand>::decode_from_ptr(
                        #crate_path::core::OperandWidth::Wide16,
                        __next_ip,
                    )
                };
                match u32::from(__opcode) {
                    #(#wide_opcode_arms,)*
                    _ => #wide16_invalid,
                }
            }
            0xFF => {
                let (__opcode, __ip) = unsafe {
                    <u32 as #crate_path::core::Operand>::decode_from_ptr(
                        #crate_path::core::OperandWidth::Wide32,
                        __next_ip,
                    )
                };
                if __opcode > u16::MAX as u32 {
                    #wide32_invalid
                } else {
                    match __opcode {
                        #(#wide_opcode_arms,)*
                        _ => #wide32_invalid,
                    }
                }
            }
            _ => {
                let __ip = __next_ip;
                match u32::from(__opcode_prefix) {
                    #(#narrow_opcode_arms,)*
                    _ => #narrow_invalid,
                }
            }
        }
    }
}

pub(crate) fn generate_tail_interpreter_invalid_expr(
    invalid_opcode_binding_var: &Ident,
    invalid_width_binding_var: &Ident,
    invalid_expr_var: &Ident,
    opcode_expr: TokenStream2,
    width_expr: TokenStream2,
) -> TokenStream2 {
    quote! {{
        let $#invalid_opcode_binding_var = #opcode_expr;
        let $#invalid_width_binding_var = #width_expr;
        $#invalid_expr_var
    }}
}

pub(crate) fn generate_tail_interpreter_inline_arm(
    opcode: &OpcodeModel,
    opcode_value: u32,
    crate_path: &TokenStream2,
) -> TokenStream2 {
    let opcode_ident = &opcode.name.ident;
    let binding_ident = format_ident!("__{}_binding", opcode.name.snake_case);
    let decoded_ident = format_ident!("__decoded_{}", opcode.name.snake_case);
    let handler_ident = format_ident!("__{}_handler", opcode.name.snake_case);
    let next_ip_binding_var = format_ident!("__next_ip_binding");
    let store_var = format_ident!("__store");

    let init_fields = opcode
        .body
        .args
        .iter()
        .map(|field| {
            let ident = &field.ident;
            quote!(#ident,)
        })
        .collect::<Vec<_>>();
    let metadata_id_init = if opcode.body.metadata.is_empty() {
        quote!()
    } else {
        quote!(metadata_id,)
    };

    let success_expr = quote! {{
        let #decoded_ident = #opcode_ident {
            #(#init_fields)*
            #metadata_id_init
        };
        let $#binding_ident = #decoded_ident;
        let $#next_ip_binding_var = __ip;
        $#store_var
        $#handler_ident
    }};

    let decode_statements = opcode
        .body
        .args
        .iter()
        .map(|field| {
            let ident = &field.ident;
            let ty = &field.ty;
            if field.optional {
                quote! {
                    let (#ident, __ip) = unsafe {
                        <#ty as #crate_path::core::OptionalOperand>::decode_optional_from_ptr(__width, __ip)
                    };
                }
            } else {
                quote! {
                    let (#ident, __ip) = unsafe {
                        <#ty as #crate_path::core::Operand>::decode_from_ptr(__width, __ip)
                    };
                }
            }
        })
        .collect::<Vec<_>>();
    let metadata_id_decode = if opcode.body.metadata.is_empty() {
        quote!()
    } else {
        quote! {
            let (metadata_id, __ip) = unsafe {
                <u32 as #crate_path::core::Operand>::decode_from_ptr(__width, __ip)
            };
        }
    };

    quote! {
        #opcode_value => {
            let __width = match __opcode_prefix {
                0xFE => #crate_path::core::OperandWidth::Wide16,
                0xFF => #crate_path::core::OperandWidth::Wide32,
                _ => #crate_path::core::OperandWidth::Narrow,
            };
            let __ip = __ip;
            #(#decode_statements)*
            #metadata_id_decode
            #success_expr
        }
    }
}

pub(crate) fn generate_expression_interpreter_decoder(
    opcode: &OpcodeModel,
    opcode_value: u32,
    crate_path: &TokenStream2,
    ret_ty: &TokenStream2,
) -> TokenStream2 {
    let opcode_ident = &opcode.name.ident;
    let binding_ident = format_ident!("__{}_binding", opcode.name.snake_case);
    let decoded_ident = format_ident!("__decoded_{}", opcode.name.snake_case);
    let handler_ident = format_ident!("__{}_handler", opcode.name.snake_case);
    let param_var = format_ident!("__param");
    let param_ty_var = format_ident!("__param_ty");
    let next_ip_binding_var = format_ident!("__next_ip_binding");
    let store_var = format_ident!("__store");

    let decode_widths = [
        ("narrow", quote!(#crate_path::core::OperandWidth::Narrow)),
        ("wide16", quote!(#crate_path::core::OperandWidth::Wide16)),
        ("wide32", quote!(#crate_path::core::OperandWidth::Wide32)),
    ];

    let action = quote!($#handler_ident);

    let binding_setup = quote! {
        let $#binding_ident = #decoded_ident;
    };

    let decode_statements = opcode
        .body
        .args
        .iter()
        .map(|field| {
            let ident = &field.ident;
            let ty = &field.ty;
            if field.optional {
                quote! {
                    let (#ident, __ip) = unsafe {
                        <#ty as #crate_path::core::OptionalOperand>::decode_optional_from_ptr(__width, __ip)
                    };
                }
            } else {
                quote! {
                    let (#ident, __ip) = unsafe {
                        <#ty as #crate_path::core::Operand>::decode_from_ptr(__width, __ip)
                    };
                }
            }
        })
        .collect::<Vec<_>>();
    let metadata_id_decode = if opcode.body.metadata.is_empty() {
        quote!()
    } else {
        quote! {
            let (metadata_id, __ip) = unsafe {
                <u32 as #crate_path::core::Operand>::decode_from_ptr(__width, __ip)
            };
        }
    };

    let init_fields = opcode
        .body
        .args
        .iter()
        .map(|field| {
            let ident = &field.ident;
            quote!(#ident,)
        })
        .collect::<Vec<_>>();
    let metadata_id_init = if opcode.body.metadata.is_empty() {
        quote!()
    } else {
        quote!(metadata_id,)
    };

    let width_decoders = decode_widths.into_iter().map(|(suffix, width)| {
        let decoder_ident = format_ident!("decode_{}_{}", opcode.name.snake_case, suffix);
        quote! {
            #[allow(unused_variables)]
            #[inline(always)]
            fn #decoder_ident($( $#param_var: $#param_ty_var, )* __ip: *const u8) -> #ret_ty {
                let __width = #width;
                let __ip = __ip;
                #(#decode_statements)*
                #metadata_id_decode
                let #decoded_ident = #opcode_ident {
                    #(#init_fields)*
                    #metadata_id_init
                };
                #binding_setup
                let $#next_ip_binding_var = __ip;
                $#store_var
                #action
            }
        }
    });

    quote! {
        const _: u32 = #opcode_value;
        #(#width_decoders)*
    }
}
