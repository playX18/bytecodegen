use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::Ident;

use crate::model::{EncodingModel, FieldModel, OpcodeModel, SchemaModel};

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
    match (schema.encoding, flavor) {
        (EncodingModel::Bytes, InterpreterMacroFlavor::Expression) => {
            generate_expression_interpreter_runtime(schema, crate_path, ret_ty)
        }
        (EncodingModel::Bytes, InterpreterMacroFlavor::TailPath) => {
            generate_tail_interpreter_runtime(schema, crate_path, ret_ty)
        }
        (EncodingModel::Packed { .. }, InterpreterMacroFlavor::Expression) => {
            generate_packed_expression_interpreter_runtime(schema, crate_path, ret_ty)
        }
        (EncodingModel::Packed { .. }, InterpreterMacroFlavor::TailPath) => {
            generate_packed_tail_interpreter_runtime(schema, crate_path, ret_ty)
        }
    }
}

pub(crate) fn generate_packed_expression_interpreter_runtime(
    schema: &SchemaModel,
    crate_path: &TokenStream2,
    _ret_ty: &TokenStream2,
) -> TokenStream2 {
    let load_var = format_ident!("__load");
    let invalid_opcode_binding_var = format_ident!("__invalid_opcode_binding");
    let invalid_width_binding_var = format_ident!("__invalid_width_binding");
    let invalid_expr_var = format_ident!("__invalid_expr");

    let opcode_arms = schema
        .opcodes
        .iter()
        .enumerate()
        .map(|(index, opcode)| {
            generate_packed_loop_interpreter_inline_arm(opcode, index as u32, crate_path)
        })
        .collect::<Vec<_>>();

    let invalid = generate_loop_interpreter_invalid_stmt(
        &invalid_opcode_binding_var,
        &invalid_width_binding_var,
        &invalid_expr_var,
        quote!(__opcode),
        quote!(#crate_path::core::OperandWidth::Wide32),
    );

    quote! {
        loop {
            let __instruction_ptr = ($#load_var) as *const u8;
            let __first_word = unsafe { __instruction_ptr.cast::<u32>().read_unaligned() };
            let __opcode = #crate_path::core::extract_packed_bits(__first_word, 0, 8);

            match __opcode {
                #(#opcode_arms,)*
                _ => #invalid,
            }
        }
    }
}

pub(crate) fn generate_packed_loop_interpreter_inline_arm(
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
    let byte_len = packed_opcode_byte_len(opcode);

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

    let decode_statements = generate_packed_decode_statements(opcode, crate_path);
    let metadata_id_decode = if opcode.body.metadata.is_empty() {
        quote!()
    } else {
        quote!(let metadata_id = 0u32;)
    };

    quote! {
        #opcode_value => {
            #decode_statements
            #metadata_id_decode
            let __ip = unsafe { __instruction_ptr.add(#byte_len) };
            let #decoded_ident = #opcode_ident {
                #(#init_fields)*
                #metadata_id_init
            };
            let $#binding_ident = #decoded_ident;
            let $#next_ip_binding_var = __ip;
            $#store_var
            $#handler_ident;
        }
    }
}

pub(crate) fn generate_expression_interpreter_runtime(
    schema: &SchemaModel,
    crate_path: &TokenStream2,
    _ret_ty: &TokenStream2,
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
            generate_loop_interpreter_inline_arm(
                opcode,
                index as u32,
                crate_path,
                quote!(#crate_path::core::OperandWidth::Narrow),
            )
        })
        .collect::<Vec<_>>();
    let wide16_opcode_arms = schema
        .opcodes
        .iter()
        .enumerate()
        .map(|(index, opcode)| {
            generate_loop_interpreter_inline_arm(
                opcode,
                index as u32,
                crate_path,
                quote!(#crate_path::core::OperandWidth::Wide16),
            )
        })
        .collect::<Vec<_>>();
    let wide32_opcode_arms = schema
        .opcodes
        .iter()
        .enumerate()
        .map(|(index, opcode)| {
            generate_loop_interpreter_inline_arm(
                opcode,
                index as u32,
                crate_path,
                quote!(#crate_path::core::OperandWidth::Wide32),
            )
        })
        .collect::<Vec<_>>();

    let narrow_invalid = generate_loop_interpreter_invalid_stmt(
        &invalid_opcode_binding_var,
        &invalid_width_binding_var,
        &invalid_expr_var,
        quote!(u32::from(__opcode_prefix)),
        quote!(#crate_path::core::OperandWidth::Narrow),
    );
    let wide16_invalid = generate_loop_interpreter_invalid_stmt(
        &invalid_opcode_binding_var,
        &invalid_width_binding_var,
        &invalid_expr_var,
        quote!(u32::from(__opcode)),
        quote!(#crate_path::core::OperandWidth::Wide16),
    );
    let wide32_invalid = generate_loop_interpreter_invalid_stmt(
        &invalid_opcode_binding_var,
        &invalid_width_binding_var,
        &invalid_expr_var,
        quote!(__opcode),
        quote!(#crate_path::core::OperandWidth::Wide32),
    );

    quote! {
        loop {
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
                        #(#wide16_opcode_arms,)*
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
                            #(#wide32_opcode_arms,)*
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
}

pub(crate) fn generate_loop_interpreter_invalid_stmt(
    invalid_opcode_binding_var: &Ident,
    invalid_width_binding_var: &Ident,
    invalid_expr_var: &Ident,
    opcode_expr: TokenStream2,
    width_expr: TokenStream2,
) -> TokenStream2 {
    quote! {{
        let $#invalid_opcode_binding_var = #opcode_expr;
        let $#invalid_width_binding_var = #width_expr;
        $#invalid_expr_var;
    }}
}

pub(crate) fn generate_loop_interpreter_inline_arm(
    opcode: &OpcodeModel,
    opcode_value: u32,
    crate_path: &TokenStream2,
    width_expr: TokenStream2,
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

    let success_stmt = quote! {{
        let #decoded_ident = #opcode_ident {
            #(#init_fields)*
            #metadata_id_init
        };
        let $#binding_ident = #decoded_ident;
        let $#next_ip_binding_var = __ip;
        $#store_var
        $#handler_ident;
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
            let __width = #width_expr;
            let __ip = __ip;
            #(#decode_statements)*
            #metadata_id_decode
            #success_stmt
        }
    }
}

pub(crate) fn generate_tail_interpreter_runtime(
    schema: &SchemaModel,
    crate_path: &TokenStream2,
    ret_ty: &TokenStream2,
) -> TokenStream2 {
    let load_var = format_ident!("__load");
    let invalid_opcode_binding_var = format_ident!("__invalid_opcode_binding");
    let invalid_width_binding_var = format_ident!("__invalid_width_binding");
    let invalid_expr_var = format_ident!("__invalid_expr");
    let param_var = format_ident!("__param");
    let param_ty_var = format_ident!("__param_ty");

    let per_opcode_dispatch_fns = schema
        .opcodes
        .iter()
        .enumerate()
        .map(|(index, opcode)| {
            generate_tail_interpreter_dispatch_fns(opcode, index, crate_path, ret_ty)
        })
        .collect::<Vec<_>>();

    let narrow_table_assignments = schema
        .opcodes
        .iter()
        .enumerate()
        .filter(|(index, _)| *index <= crate::MAX_NARROW_OPCODE as usize)
        .map(|(index, opcode)| {
            let handler_ident = format_ident!("__dispatch_{}_narrow", opcode.name.snake_case);
            quote! {
                __table[#index] = #handler_ident as __DispatchFn;
            }
        })
        .collect::<Vec<_>>();

    let wide16_table_assignments = schema
        .opcodes
        .iter()
        .enumerate()
        .map(|(index, opcode)| {
            let handler_ident = format_ident!("__dispatch_{}_wide16", opcode.name.snake_case);
            quote! {
                __table[#index] = #handler_ident as __DispatchFn;
            }
        })
        .collect::<Vec<_>>();

    let wide32_table_assignments = schema
        .opcodes
        .iter()
        .enumerate()
        .map(|(index, opcode)| {
            let handler_ident = format_ident!("__dispatch_{}_wide32", opcode.name.snake_case);
            quote! {
                __table[#index] = #handler_ident as __DispatchFn;
            }
        })
        .collect::<Vec<_>>();

    let invalid_narrow_body = generate_tail_interpreter_invalid_expr(
        &invalid_opcode_binding_var,
        &invalid_width_binding_var,
        &invalid_expr_var,
        quote!(u32::from(__opcode)),
        quote!(#crate_path::core::OperandWidth::Narrow),
    );
    let invalid_wide16_body = generate_tail_interpreter_invalid_expr(
        &invalid_opcode_binding_var,
        &invalid_width_binding_var,
        &invalid_expr_var,
        quote!(u32::from(__opcode)),
        quote!(#crate_path::core::OperandWidth::Wide16),
    );
    let invalid_wide32_body = generate_tail_interpreter_invalid_expr(
        &invalid_opcode_binding_var,
        &invalid_width_binding_var,
        &invalid_expr_var,
        quote!(__opcode),
        quote!(#crate_path::core::OperandWidth::Wide32),
    );

    quote! {
        type __DispatchFn = fn($( $#param_ty_var, )*) -> #ret_ty;

        #[allow(unused_variables)]
        #[inline(always)]
        fn __invalid_narrow($( $#param_var: $#param_ty_var, )*) -> #ret_ty {
            let __instruction_ptr = ($#load_var) as *const u8;
            let (__opcode, _) = unsafe {
                <u8 as #crate_path::core::Operand>::decode_from_ptr(
                    #crate_path::core::OperandWidth::Narrow,
                    __instruction_ptr,
                )
            };
            #invalid_narrow_body
        }

        #[allow(unused_variables)]
        #[inline(always)]
        fn __invalid_wide16($( $#param_var: $#param_ty_var, )*) -> #ret_ty {
            let __instruction_ptr = ($#load_var) as *const u8;
            let __opcode_ptr = unsafe { __instruction_ptr.add(1) };
            let (__opcode, _) = unsafe {
                <u16 as #crate_path::core::Operand>::decode_from_ptr(
                    #crate_path::core::OperandWidth::Wide16,
                    __opcode_ptr,
                )
            };
            #invalid_wide16_body
        }

        #[allow(unused_variables)]
        #[inline(always)]
        fn __invalid_wide32($( $#param_var: $#param_ty_var, )*) -> #ret_ty {
            let __instruction_ptr = ($#load_var) as *const u8;
            let __opcode_ptr = unsafe { __instruction_ptr.add(1) };
            let (__opcode, _) = unsafe {
                <u32 as #crate_path::core::Operand>::decode_from_ptr(
                    #crate_path::core::OperandWidth::Wide32,
                    __opcode_ptr,
                )
            };
            #invalid_wide32_body
        }

        #[allow(unused_variables)]
        #[inline(always)]
        fn __dispatch($( $#param_var: $#param_ty_var, )*) -> #ret_ty {
            let __instruction_ptr = ($#load_var) as *const u8;
            unsafe {
                let __opcode_prefix = __instruction_ptr.read();
                let __entry = *__DISPATCH_TABLE_NARROW.get_unchecked(__opcode_prefix as usize);
                become __entry($( $#param_var, )*)
            }
        }

        #[allow(unused_variables)]
        #[inline(always)]
        fn __dispatch_wide16($( $#param_var: $#param_ty_var, )*) -> #ret_ty {
            let __instruction_ptr = ($#load_var) as *const u8;
            unsafe {
                let __opcode_ptr =  __instruction_ptr.add(1);
                let __opcode = __opcode_ptr.cast::<u16>().read_unaligned();
                let __entry = *__DISPATCH_TABLE_WIDE16.get_unchecked(__opcode as usize);
                become __entry($( $#param_var, )*)
            }
        }

        #[allow(unused_variables)]
        #[inline(always)]
        fn __dispatch_wide32($( $#param_var: $#param_ty_var, )*) -> #ret_ty {
            let __instruction_ptr = ($#load_var) as *const u8;
            unsafe {
                let __opcode_ptr =  __instruction_ptr.add(1);
                let __opcode = __opcode_ptr.cast::<u32>().read_unaligned();
                let __opcode_index = __opcode as usize;
            
                debug_assert!(__opcode_index < __DISPATCH_TABLE_WIDE16.len());
                let __entry = *__DISPATCH_TABLE_WIDE32.get_unchecked(__opcode_index);
                
                become __entry($( $#param_var, )*)
            
            }
        }

        #(#per_opcode_dispatch_fns)*

        const fn __build_dispatch_table_narrow() -> [__DispatchFn; 256] {
            let mut __table = [__invalid_narrow as __DispatchFn; 256];
            #(#narrow_table_assignments)*
            __table[0xFE] = __dispatch_wide16 as __DispatchFn;
            __table[0xFF] = __dispatch_wide32 as __DispatchFn;
            __table
        }

        const fn __build_dispatch_table_wide16() -> [__DispatchFn; 65536] {
            let mut __table = [__invalid_wide16 as __DispatchFn; 65536];
            #(#wide16_table_assignments)*
            __table
        }

        const fn __build_dispatch_table_wide32() -> [__DispatchFn; 65536] {
            let mut __table = [__invalid_wide32 as __DispatchFn; 65536];
            #(#wide32_table_assignments)*
            __table
        }

        static __DISPATCH_TABLE_NARROW: [__DispatchFn; 256] = __build_dispatch_table_narrow();
        static __DISPATCH_TABLE_WIDE16: [__DispatchFn; 65536] = __build_dispatch_table_wide16();
        static __DISPATCH_TABLE_WIDE32: [__DispatchFn; 65536] = __build_dispatch_table_wide32();

        become __dispatch($( $#param_var, )*)
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

pub(crate) fn generate_tail_interpreter_dispatch_fns(
    opcode: &OpcodeModel,
    _opcode_value: usize,
    crate_path: &TokenStream2,
    ret_ty: &TokenStream2,
) -> TokenStream2 {
    let opcode_ident = &opcode.name.ident;
    let binding_ident = format_ident!("__{}_binding", opcode.name.snake_case);
    let decoded_ident = format_ident!("__decoded_{}", opcode.name.snake_case);
    let handler_ident = format_ident!("__{}_handler", opcode.name.snake_case);
    let load_var = format_ident!("__load");
    let param_var = format_ident!("__param");
    let param_ty_var = format_ident!("__param_ty");
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

    let success_expr = quote! {
        let #decoded_ident = #opcode_ident {
            #(#init_fields)*
            #metadata_id_init
        };
        let $#binding_ident = #decoded_ident;
        let $#next_ip_binding_var = __ip;
        $#store_var
        $#handler_ident
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

    let width_dispatch_fns = [
        (
            format_ident!("__dispatch_{}_narrow", opcode.name.snake_case),
            quote!(#crate_path::core::OperandWidth::Narrow),
            quote!(1usize),
        ),
        (
            format_ident!("__dispatch_{}_wide16", opcode.name.snake_case),
            quote!(#crate_path::core::OperandWidth::Wide16),
            quote!(3usize),
        ),
        (
            format_ident!("__dispatch_{}_wide32", opcode.name.snake_case),
            quote!(#crate_path::core::OperandWidth::Wide32),
            quote!(5usize),
        ),
    ]
    .into_iter()
    .map(|(dispatch_ident, width_expr, header_len)| {
        quote! {
            #[allow(unused_variables, unused_mut, unused_assignments)]
            #[inline(always)]
            fn #dispatch_ident($( mut $#param_var: $#param_ty_var, )*) -> #ret_ty {
                let __instruction_ptr = ($#load_var) as *const u8;
                let __width = #width_expr;
                let __ip = unsafe { __instruction_ptr.add(#header_len) };
                #(#decode_statements)*
                #metadata_id_decode
                #success_expr
            }
        }
    });

    quote! {
        #(#width_dispatch_fns)*
    }
}

pub(crate) fn generate_packed_tail_interpreter_runtime(
    schema: &SchemaModel,
    crate_path: &TokenStream2,
    ret_ty: &TokenStream2,
) -> TokenStream2 {
    let load_var = format_ident!("__load");
    let invalid_opcode_binding_var = format_ident!("__invalid_opcode_binding");
    let invalid_width_binding_var = format_ident!("__invalid_width_binding");
    let invalid_expr_var = format_ident!("__invalid_expr");
    let param_var = format_ident!("__param");
    let param_ty_var = format_ident!("__param_ty");

    let per_opcode_dispatch_fns = schema
        .opcodes
        .iter()
        .enumerate()
        .map(|(index, opcode)| {
            generate_packed_tail_interpreter_dispatch_fn(opcode, index, crate_path, ret_ty)
        })
        .collect::<Vec<_>>();

    let table_assignments = schema
        .opcodes
        .iter()
        .enumerate()
        .map(|(index, opcode)| {
            let handler_ident = format_ident!("__dispatch_{}_packed", opcode.name.snake_case);
            quote! {
                __table[#index] = #handler_ident as __DispatchFn;
            }
        })
        .collect::<Vec<_>>();

    let invalid_body = generate_tail_interpreter_invalid_expr(
        &invalid_opcode_binding_var,
        &invalid_width_binding_var,
        &invalid_expr_var,
        quote!(__opcode),
        quote!(#crate_path::core::OperandWidth::Wide32),
    );

    quote! {
        type __DispatchFn = fn($( $#param_ty_var, )*) -> #ret_ty;

        #[allow(unused_variables)]
        #[inline(always)]
        fn __invalid_packed($( $#param_var: $#param_ty_var, )*) -> #ret_ty {
            let __instruction_ptr = ($#load_var) as *const u8;
            let __first_word = unsafe { __instruction_ptr.cast::<u32>().read_unaligned() };
            let __opcode = #crate_path::core::extract_packed_bits(__first_word, 0, 8);
            #invalid_body
        }

        #[allow(unused_variables)]
        #[inline(always)]
        fn __dispatch($( $#param_var: $#param_ty_var, )*) -> #ret_ty {
            let __instruction_ptr = ($#load_var) as *const u8;
            unsafe {
                let __first_word = __instruction_ptr.cast::<u32>().read_unaligned();
                let __opcode = #crate_path::core::extract_packed_bits(__first_word, 0, 8);
                let __entry = *__DISPATCH_TABLE_PACKED.get_unchecked(__opcode as usize);
                become __entry($( $#param_var, )*)
            }
        }

        #(#per_opcode_dispatch_fns)*

        const fn __build_dispatch_table_packed() -> [__DispatchFn; 256] {
            let mut __table = [__invalid_packed as __DispatchFn; 256];
            #(#table_assignments)*
            __table
        }

        static __DISPATCH_TABLE_PACKED: [__DispatchFn; 256] = __build_dispatch_table_packed();

        become __dispatch($( $#param_var, )*)
    }
}

pub(crate) fn generate_packed_tail_interpreter_dispatch_fn(
    opcode: &OpcodeModel,
    _opcode_value: usize,
    crate_path: &TokenStream2,
    ret_ty: &TokenStream2,
) -> TokenStream2 {
    let opcode_ident = &opcode.name.ident;
    let binding_ident = format_ident!("__{}_binding", opcode.name.snake_case);
    let decoded_ident = format_ident!("__decoded_{}", opcode.name.snake_case);
    let handler_ident = format_ident!("__{}_handler", opcode.name.snake_case);
    let load_var = format_ident!("__load");
    let param_var = format_ident!("__param");
    let param_ty_var = format_ident!("__param_ty");
    let next_ip_binding_var = format_ident!("__next_ip_binding");
    let store_var = format_ident!("__store");
    let dispatch_ident = format_ident!("__dispatch_{}_packed", opcode.name.snake_case);
    let byte_len = packed_opcode_byte_len(opcode);

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

    let decode_statements = generate_packed_decode_statements(opcode, crate_path);
    let metadata_id_decode = if opcode.body.metadata.is_empty() {
        quote!()
    } else {
        quote!(let metadata_id = 0u32;)
    };

    quote! {
        #[allow(unused_variables, unused_mut, unused_assignments)]
        #[inline(always)]
        fn #dispatch_ident($( mut $#param_var: $#param_ty_var, )*) -> #ret_ty {
            let __instruction_ptr = ($#load_var) as *const u8;
            #decode_statements
            #metadata_id_decode
            let __ip = unsafe { __instruction_ptr.add(#byte_len) };
            let #decoded_ident = #opcode_ident {
                #(#init_fields)*
                #metadata_id_init
            };
            let $#binding_ident = #decoded_ident;
            let $#next_ip_binding_var = __ip;
            $#store_var
            $#handler_ident
        }
    }
}

fn packed_opcode_byte_len(opcode: &OpcodeModel) -> usize {
    opcode
        .packed_layout
        .as_ref()
        .expect("packed interpreter requires packed opcode layouts")
        .word_count
        * 4
}

fn generate_packed_decode_statements(
    opcode: &OpcodeModel,
    crate_path: &TokenStream2,
) -> TokenStream2 {
    let layout = opcode
        .packed_layout
        .as_ref()
        .expect("packed interpreter requires packed opcode layouts");

    let word_loads = (0..layout.word_count).map(|word_index| {
        let word_ident = packed_word_ident(word_index);
        let byte_offset = word_index * 4;
        quote! {
            let #word_ident = unsafe {
                __instruction_ptr.add(#byte_offset).cast::<u32>().read_unaligned()
            };
        }
    });

    let raw_decls = opcode.body.args.iter().map(|field| {
        let raw_ident = packed_raw_ident(field);
        quote!(let mut #raw_ident = 0u32;)
    });

    let mut operand_layout_bits = std::collections::HashMap::<String, u8>::new();
    for placement in &layout.placements {
        if let bytecodegen_core::PackedFieldPlacement::Operand {
            name, bit_width, ..
        } = placement
        {
            *operand_layout_bits.entry(name.clone()).or_insert(0) += bit_width;
        }
    }

    let decode_placements = layout
        .placements
        .iter()
        .filter_map(|placement| match placement {
            bytecodegen_core::PackedFieldPlacement::Opcode { .. } => None,
            bytecodegen_core::PackedFieldPlacement::Operand {
                name,
                word_index,
                bit_offset,
                bit_width,
                operand_bit_offset,
            } => {
                let raw_ident = packed_raw_ident(find_field(opcode, name));
                let word_ident = packed_word_ident(*word_index);
                Some(quote! {
                    #raw_ident |= #crate_path::core::extract_packed_bits(
                        #word_ident,
                        #bit_offset,
                        #bit_width,
                    ) << #operand_bit_offset;
                })
            }
        });

    let decode_fields = opcode.body.args.iter().map(|field| {
        let raw_ident = packed_raw_ident(field);
        let field_ident = &field.ident;
        let width = operand_layout_bits
            .get(&field.ident.to_string())
            .copied()
            .unwrap_or(0);
        let value_expr = packed_fast_decode_value_expr(field, &raw_ident, width, crate_path);
        if field.optional {
            quote! {
                let #field_ident = if #raw_ident == 0 {
                    None
                } else {
                    Some(#value_expr)
                };
            }
        } else {
            quote! {
                let #field_ident = #value_expr;
            }
        }
    });

    quote! {
        #(#word_loads)*
        #(#raw_decls)*
        #(#decode_placements)*
        #(#decode_fields)*
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

fn packed_word_ident(word_index: usize) -> Ident {
    format_ident!("__packed_word_{word_index}")
}

fn packed_fast_decode_value_expr(
    field: &FieldModel,
    raw_ident: &Ident,
    width: u8,
    crate_path: &TokenStream2,
) -> TokenStream2 {
    let ty = &field.ty;
    let normalized_ty = quote!(#ty).to_string().replace(' ', "");
    match normalized_ty.as_str() {
        "bool" => quote!(#raw_ident != 0),
        "u8" | "u16" | "u32" | "usize" => quote!(#raw_ident as #ty),
        "i8" | "i16" | "i32" | "isize" => {
            quote!(#crate_path::core::sign_extend_packed(#raw_ident, #width) as #ty)
        }
        _ => {
            let message = format!("packed operand `{}` failed to decode", field.ident);
            quote! {
                <#ty as #crate_path::core::PackedOperand>::decode_packed_width(
                    #raw_ident,
                    #width,
                ).expect(#message)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro2::Span;
    use quote::quote;

    use crate::doc::DocBlock;
    use crate::model::{
        BodyModel, EncodingModel, NameModel, OpcodeModel, SchemaModel, SectionModel,
    };

    fn name(raw: &str, require_upper_camel: bool) -> NameModel {
        NameModel::from_ident(
            Ident::new(raw, Span::call_site()),
            require_upper_camel,
            "test",
        )
        .unwrap()
    }

    fn opcode(raw: &str) -> OpcodeModel {
        OpcodeModel {
            docs: DocBlock::default(),
            name: name(raw, true),
            section: name("Bytecode", false),
            body: BodyModel {
                args: Vec::new(),
                metadata: Vec::new(),
                tmps: Vec::new(),
                checkpoints: Vec::new(),
                layout: None,
            },
            verify: None,
            packed_layout: None,
        }
    }

    fn packed_opcode(raw: &str, opcode_value: usize) -> OpcodeModel {
        let mut opcode = opcode(raw);
        opcode.packed_layout = Some(bytecodegen_core::PackedLayoutPlan {
            word_bits: 32,
            word_count: 1,
            placements: vec![bytecodegen_core::PackedFieldPlacement::opcode(0, 0, 8)],
        });
        assert!(opcode_value <= u8::MAX as usize);
        opcode
    }

    fn schema() -> SchemaModel {
        let opcodes = vec![opcode("Noop"), opcode("Return")];
        SchemaModel {
            encoding: EncodingModel::Bytes,
            sections: vec![SectionModel {
                docs: DocBlock::default(),
                name: name("Bytecode", false),
                preserve_order: true,
                debug: None,
                source_items: Vec::new(),
                opcodes: opcodes.clone(),
            }],
            opcodes,
            groups: Vec::new(),
        }
    }

    fn packed_schema() -> SchemaModel {
        let opcodes = vec![packed_opcode("Noop", 0), packed_opcode("Return", 1)];
        SchemaModel {
            encoding: EncodingModel::Packed { word_bits: 32 },
            sections: vec![SectionModel {
                docs: DocBlock::default(),
                name: name("Bytecode", false),
                preserve_order: true,
                debug: None,
                source_items: Vec::new(),
                opcodes: opcodes.clone(),
            }],
            opcodes,
            groups: Vec::new(),
        }
    }

    #[test]
    fn tail_interpreter_dispatch_uses_static_tables() {
        let runtime =
            generate_tail_interpreter_runtime(&schema(), &quote!(bytecodegen), &quote!(()))
                .to_string();

        assert!(runtime.contains("static __DISPATCH_TABLE_NARROW"));
        assert!(runtime.contains("static __DISPATCH_TABLE_WIDE16"));
        assert!(runtime.contains("static __DISPATCH_TABLE_WIDE32"));
        assert!(runtime.contains("become __entry"));
        assert!(!runtime.contains("match __opcode_prefix"));
    }

    #[test]
    fn expression_interpreter_specializes_arm_widths() {
        let runtime =
            generate_expression_interpreter_runtime(&schema(), &quote!(bytecodegen), &quote!(()))
                .to_string();

        assert!(!runtime.contains("let __width = match __opcode_prefix"));
        assert!(runtime.contains("OperandWidth :: Narrow"));
        assert!(runtime.contains("OperandWidth :: Wide16"));
        assert!(runtime.contains("OperandWidth :: Wide32"));
    }

    #[test]
    fn tail_interpreter_dispatch_keeps_user_signature() {
        let runtime =
            generate_tail_interpreter_runtime(&schema(), &quote!(bytecodegen), &quote!(()))
                .to_string();

        assert!(!runtime.contains("type __DispatchFn = fn (* const u8"));
        assert!(!runtime.contains("become __entry (__instruction_ptr"));
        assert!(!runtime.contains("fn __dispatch_noop_narrow (__instruction_ptr"));
        assert!(runtime.contains("fn __dispatch_noop_narrow ("));
    }

    #[test]
    fn tail_interpreter_checks_wide32_table_bounds() {
        let runtime =
            generate_tail_interpreter_runtime(&schema(), &quote!(bytecodegen), &quote!(()))
                .to_string();

        assert!(runtime.contains("__opcode_index < __DISPATCH_TABLE_WIDE32 . len ()"));
        assert!(runtime.contains("else { become __invalid_wide32"));
        assert!(!runtime.contains("become __invalid_wide32 (__instruction_ptr"));
    }

    #[test]
    fn packed_expression_interpreter_decodes_packed_words() {
        let runtime = generate_interpreter_runtime(
            &packed_schema(),
            &quote!(bytecodegen),
            InterpreterMacroFlavor::Expression,
            &quote!(()),
        )
        .to_string();

        assert!(runtime.contains("read_unaligned"));
        assert!(runtime.contains("extract_packed_bits"));
        assert!(!runtime.contains("decode_from_ptr"));
    }

    #[test]
    fn packed_tail_interpreter_uses_packed_dispatch_table() {
        let runtime = generate_interpreter_runtime(
            &packed_schema(),
            &quote!(bytecodegen),
            InterpreterMacroFlavor::TailPath,
            &quote!(()),
        )
        .to_string();

        assert!(runtime.contains("static __DISPATCH_TABLE_PACKED"));
        assert!(runtime.contains("become __entry"));
        assert!(!runtime.contains("become __entry (__instruction_ptr"));
        assert!(!runtime.contains("type __DispatchFn = fn (* const u8"));
        assert!(!runtime.contains("__DISPATCH_TABLE_WIDE16"));
    }
}
