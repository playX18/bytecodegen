use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    Data, DataEnum, DeriveInput, Error, Expr, Fields, Ident, LitInt, Result, Type, parse::Parse,
    parse::ParseStream, token::Eq,
};

// ---------------------------------------------------------------------------
// Helper: parse #[operand(max = N)]
// ---------------------------------------------------------------------------

struct OperandAttrs {
    max: Option<Expr>,
}

struct MaxAttr {
    _key: Ident,
    _eq: Eq,
    value: Expr,
}

impl Parse for MaxAttr {
    fn parse(input: ParseStream) -> Result<Self> {
        let _key: Ident = input.parse()?;
        let _eq: Eq = input.parse()?;
        let value: Expr = input.parse()?;
        Ok(Self { _key, _eq, value })
    }
}

fn parse_operand_attrs(input: &DeriveInput) -> Result<OperandAttrs> {
    let mut max = None;
    for attr in &input.attrs {
        if attr.path().is_ident("operand") {
            let parsed: MaxAttr = attr.parse_args()?;
            max = Some(parsed.value);
        }
    }
    Ok(OperandAttrs { max })
}

// ---------------------------------------------------------------------------
// Helper: extract repr type from #[repr(u8)], #[repr(u16)], #[repr(u32)]
// ---------------------------------------------------------------------------

fn extract_repr_type(input: &DeriveInput) -> Result<Option<Type>> {
    for attr in &input.attrs {
        if attr.path().is_ident("repr") {
            let tokens = attr.meta.require_list()?.tokens.clone();
            let repr: Type = syn::parse2(tokens)?;
            let repr_str = quote!(#repr).to_string();
            match repr_str.as_str() {
                "u8" | "u16" | "u32" | "i8" | "i16" | "i32" => return Ok(Some(repr)),
                _ => {
                    return Err(Error::new_spanned(
                        attr,
                        "Operand derive only supports #[repr(u8)], #[repr(u16)], #[repr(u32)], #[repr(i8)], #[repr(i16)], or #[repr(i32)]",
                    ));
                }
            }
        }
    }
    Ok(None)
}

// ---------------------------------------------------------------------------
// Helper: check if a type is a numeric primitive eligible for RelocationField
// ---------------------------------------------------------------------------

fn is_numeric_repr(ty: &Type) -> bool {
    let s = quote!(#ty).to_string();
    matches!(
        s.as_str(),
        "u8" | "u16" | "u32" | "i8" | "i16" | "i32" | "usize" | "isize"
    )
}

// ---------------------------------------------------------------------------
// Helper: extract inner type from a newtype struct
// ---------------------------------------------------------------------------

fn extract_newtype_inner(fields: &Fields) -> Result<&Type> {
    match fields {
        Fields::Unnamed(f) if f.unnamed.len() == 1 => Ok(&f.unnamed.first().unwrap().ty),
        _ => Err(Error::new_spanned(
            fields,
            "Operand derive on structs requires a newtype (single unnamed field)",
        )),
    }
}

// ---------------------------------------------------------------------------
// Discriminant info for enum variants
// ---------------------------------------------------------------------------

struct VariantInfo {
    /// The variant identifier.
    ident: Ident,
    /// Const ident for the discriminant value (usable in match patterns).
    disc_const: Ident,
    /// Token stream for the discriminant value expression.
    disc_expr: TokenStream2,
}

/// Build variant info for all enum variants.
fn build_variant_infos(
    enum_name: &Ident,
    data: &DataEnum,
    repr: &Type,
) -> Result<Vec<VariantInfo>> {
    let enum_upper = enum_name.to_string().to_uppercase();
    let mut infos = Vec::new();
    for (i, variant) in data.variants.iter().enumerate() {
        let var_upper = variant.ident.to_string().to_uppercase();
        let disc_const = format_ident!("__{enum_upper}_{var_upper}_DISC");
        let disc_expr = match &variant.discriminant {
            Some((_, expr)) => quote!(#expr as #repr),
            None => {
                let lit = LitInt::new(&i.to_string(), variant.ident.span());
                quote!(#lit as #repr)
            }
        };
        infos.push(VariantInfo {
            ident: variant.ident.clone(),
            disc_const,
            disc_expr,
        });
    }
    Ok(infos)
}

/// Generate const definitions for discriminant values.
fn gen_discriminant_consts(variant_infos: &[VariantInfo], repr: &Type) -> TokenStream2 {
    let defs = variant_infos.iter().map(|vi| {
        let c = &vi.disc_const;
        let e = &vi.disc_expr;
        quote! { const #c: #repr = #e; }
    });
    quote! { #(#defs)* }
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

pub(crate) fn derive_operand(input: DeriveInput) -> TokenStream2 {
    match derive_operand_inner(input) {
        Ok(tokens) => tokens,
        Err(err) => err.to_compile_error(),
    }
}

fn derive_operand_inner(input: DeriveInput) -> Result<TokenStream2> {
    let crate_path = crate::build::bytecodegen_path()?;
    let name = &input.ident;
    let operand_attrs = parse_operand_attrs(&input)?;

    let (operand_impl, optional_impl, reloc_impl) = match &input.data {
        Data::Enum(data) => {
            let repr = extract_repr_type(&input)?
                .ok_or_else(|| Error::new_spanned(&input, "Operand derive on enums requires a #[repr(u8)], #[repr(u16)], or #[repr(u32)] attribute"))?;
            let variant_infos = build_variant_infos(name, data, &repr)?;
            let enum_impl = gen_enum_operand(&crate_path, name, &variant_infos, &repr)?;
            let optional_impl =
                gen_optional_operand_enum(&crate_path, name, &variant_infos, &repr)?;
            let reloc_impl = if is_numeric_repr(&repr) {
                Some(gen_enum_relocation_field(
                    &crate_path,
                    name,
                    &variant_infos,
                    &repr,
                )?)
            } else {
                None
            };
            (enum_impl, Some(optional_impl), reloc_impl)
        }
        Data::Struct(data) => {
            let inner_type = extract_newtype_inner(&data.fields)?;
            let newtype_impl = gen_newtype_operand(&crate_path, name, inner_type, &operand_attrs)?;
            let optional_impl = gen_optional_operand_newtype(&crate_path, name, inner_type)?;
            let reloc_impl = if is_numeric_repr(inner_type) {
                Some(gen_newtype_relocation_field(&crate_path, name, inner_type)?)
            } else {
                None
            };
            (newtype_impl, Some(optional_impl), reloc_impl)
        }
        _ => {
            return Err(Error::new_spanned(
                &input,
                "Operand derive only supports enums and newtype structs",
            ));
        }
    };

    let mut output = quote! {
        #operand_impl
    };

    if let Some(opt) = optional_impl {
        output = quote! {
            #output
            #opt
        };
    }

    if let Some(reloc) = reloc_impl {
        output = quote! {
            #output
            #reloc
        };
    }

    Ok(output)
}

// ---------------------------------------------------------------------------
// Enum Operand impl
// ---------------------------------------------------------------------------

fn gen_enum_operand(
    crate_path: &TokenStream2,
    name: &Ident,
    variant_infos: &[VariantInfo],
    repr: &Type,
) -> Result<TokenStream2> {
    let const_defs = gen_discriminant_consts(variant_infos, repr);
    let match_arms: Vec<TokenStream2> = variant_infos
        .iter()
        .map(|vi| {
            let c = &vi.disc_const;
            let v = &vi.ident;
            quote! { #c => Ok(#name::#v) }
        })
        .collect();
    let ptr_match_arms: Vec<TokenStream2> = variant_infos
        .iter()
        .map(|vi| {
            let c = &vi.disc_const;
            let v = &vi.ident;
            quote! { #c => (#name::#v, next) }
        })
        .collect();

    Ok(quote! {
        #[allow(non_upper_case_globals, unreachable_patterns)]
        impl #crate_path::Operand for #name {
            fn fits_width(value: &Self, width: #crate_path::OperandWidth) -> bool {
                #crate_path::Operand::fits_width(&(*value as #repr), width)
            }

            fn encode<S: #crate_path::ByteSink + ?Sized>(value: &Self, width: #crate_path::OperandWidth, output: &mut S) {
                #crate_path::Operand::encode(&(*value as #repr), width, output)
            }

            fn decode(width: #crate_path::OperandWidth, input: &mut #crate_path::ByteCursor<'_>) -> Result<Self, #crate_path::DecodeError> {
                #const_defs
                let val = <#repr as #crate_path::Operand>::decode(width, input)?;
                match val {
                    #(#match_arms,)*
                    _ => Err(#crate_path::DecodeError::InvalidValue(stringify!(#name))),
                }
            }

            fn can_decode_from_ptr() -> bool { true }

            unsafe fn decode_from_ptr(width: #crate_path::OperandWidth, input: *const u8) -> (Self, *const u8) {
                #const_defs
                let (val, next) = <#repr as #crate_path::Operand>::decode_from_ptr(width, input);
                match val {
                    #(#ptr_match_arms,)*
                    _ => unreachable!("invalid discriminant for {}", stringify!(#name)),
                }
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Enum OptionalOperand impl
// ---------------------------------------------------------------------------

fn gen_optional_operand_enum(
    crate_path: &TokenStream2,
    name: &Ident,
    variant_infos: &[VariantInfo],
    repr: &Type,
) -> Result<TokenStream2> {
    let const_defs = gen_discriminant_consts(variant_infos, repr);
    let match_arms: Vec<TokenStream2> = variant_infos
        .iter()
        .map(|vi| {
            let c = &vi.disc_const;
            let v = &vi.ident;
            quote! { #c => Ok(Some(#name::#v)) }
        })
        .collect();
    let ptr_match_arms: Vec<TokenStream2> = variant_infos
        .iter()
        .map(|vi| {
            let c = &vi.disc_const;
            let v = &vi.ident;
            quote! { #c => (Some(#name::#v), next) }
        })
        .collect();

    Ok(quote! {
        #[allow(non_upper_case_globals, unreachable_patterns)]
        impl #crate_path::OptionalOperand for #name {
            fn encode_optional<S: #crate_path::ByteSink + ?Sized>(
                value: Option<&Self>,
                width: #crate_path::OperandWidth,
                output: &mut S,
            ) {
                match value {
                    Some(v) => <Self as #crate_path::Operand>::encode(v, width, output),
                    None => <#repr as #crate_path::Operand>::encode(&(0 as #repr), width, output),
                }
            }

            fn decode_optional(
                width: #crate_path::OperandWidth,
                input: &mut #crate_path::ByteCursor<'_>,
            ) -> Result<Option<Self>, #crate_path::DecodeError> {
                #const_defs
                let val = <#repr as #crate_path::Operand>::decode(width, input)?;
                match val {
                    0 => Ok(None),
                    #(#match_arms,)*
                    _ => Err(#crate_path::DecodeError::InvalidValue(stringify!(#name))),
                }
            }

            fn fits_optional(value: Option<&Self>, width: #crate_path::OperandWidth) -> bool {
                match value {
                    Some(v) => <Self as #crate_path::Operand>::fits_width(v, width),
                    None => true,
                }
            }

            fn can_decode_from_ptr_optional() -> bool { true }

            unsafe fn decode_optional_from_ptr(
                width: #crate_path::OperandWidth,
                input: *const u8,
            ) -> (Option<Self>, *const u8) {
                #const_defs
                let (val, next) = <#repr as #crate_path::Operand>::decode_from_ptr(width, input);
                match val {
                    0 => (None, next),
                    #(#ptr_match_arms,)*
                    _ => unreachable!("invalid discriminant for optional {}", stringify!(#name)),
                }
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Enum RelocationField impl
// ---------------------------------------------------------------------------

fn gen_enum_relocation_field(
    crate_path: &TokenStream2,
    name: &Ident,
    variant_infos: &[VariantInfo],
    repr: &Type,
) -> Result<TokenStream2> {
    let const_defs = gen_discriminant_consts(variant_infos, repr);
    let match_arms: Vec<TokenStream2> = variant_infos
        .iter()
        .map(|vi| {
            let c = &vi.disc_const;
            let v = &vi.ident;
            quote! { #c => Ok(#name::#v) }
        })
        .collect();

    Ok(quote! {
        #[allow(non_upper_case_globals)]
        impl #crate_path::RelocationField for #name {
            const TYPE_NAME: &'static str = stringify!(#name);

            fn from_relocation_value(value: i128) -> Result<Self, #crate_path::LinkError> {
                #const_defs
                let v = <#repr as #crate_path::RelocationField>::from_relocation_value(value)?;
                match v {
                    #(#match_arms,)*
                    _ => Err(#crate_path::LinkError::RelocationValueOutOfRange {
                        type_name: Self::TYPE_NAME,
                        value,
                    }),
                }
            }

            fn to_relocation_value(&self) -> i128 {
                (*self as #repr) as i128
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Newtype Operand impl
// ---------------------------------------------------------------------------

fn gen_newtype_operand(
    crate_path: &TokenStream2,
    name: &Ident,
    inner_type: &Type,
    operand_attrs: &OperandAttrs,
) -> Result<TokenStream2> {
    let fits_width_body = if let Some(max) = &operand_attrs.max {
        quote! {
            #crate_path::Operand::fits_width(&value.0, width) && value.0 <= #max
        }
    } else {
        quote! {
            #crate_path::Operand::fits_width(&value.0, width)
        }
    };

    Ok(quote! {
        impl #crate_path::Operand for #name {
            fn fits_width(value: &Self, width: #crate_path::OperandWidth) -> bool {
                #fits_width_body
            }

            fn encode<S: #crate_path::ByteSink + ?Sized>(value: &Self, width: #crate_path::OperandWidth, output: &mut S) {
                #crate_path::Operand::encode(&value.0, width, output)
            }

            fn decode(width: #crate_path::OperandWidth, input: &mut #crate_path::ByteCursor<'_>) -> Result<Self, #crate_path::DecodeError> {
                <#inner_type as #crate_path::Operand>::decode(width, input).map(Self)
            }

            fn can_decode_from_ptr() -> bool {
                <#inner_type as #crate_path::Operand>::can_decode_from_ptr()
            }

            unsafe fn decode_from_ptr(width: #crate_path::OperandWidth, input: *const u8) -> (Self, *const u8) {
                let (val, next) = <#inner_type as #crate_path::Operand>::decode_from_ptr(width, input);
                (Self(val), next)
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Newtype OptionalOperand impl
// ---------------------------------------------------------------------------

fn gen_optional_operand_newtype(
    crate_path: &TokenStream2,
    name: &Ident,
    inner_type: &Type,
) -> Result<TokenStream2> {
    // Generates OptionalOperand for newtypes using the convention that
    // a zero-encoded value represents None. Numeric types use 0 as sentinel.
    // We use Default to get the zero value safely for all supported inner types.
    Ok(quote! {
        impl #crate_path::OptionalOperand for #name {
            fn encode_optional<S: #crate_path::ByteSink + ?Sized>(
                value: Option<&Self>,
                width: #crate_path::OperandWidth,
                output: &mut S,
            ) {
                match value {
                    Some(v) => <Self as #crate_path::Operand>::encode(v, width, output),
                    None => {
                        // Encode a zero value by writing zero bytes
                        for _ in 0..width.bytes() {
                            output.push(0u8);
                        }
                    }
                }
            }

            fn decode_optional(
                width: #crate_path::OperandWidth,
                input: &mut #crate_path::ByteCursor<'_>,
            ) -> Result<Option<Self>, #crate_path::DecodeError> {
                let val = <#inner_type as #crate_path::Operand>::decode(width, input)?;
                if val == <#inner_type as Default>::default() {
                    // Ambiguity: a legitimate zero value and "none" look the same.
                    // This matches the convention used throughout the bytecode system.
                    Ok(None)
                } else {
                    Ok(Some(Self(val)))
                }
            }

            fn fits_optional(value: Option<&Self>, width: #crate_path::OperandWidth) -> bool {
                match value {
                    Some(v) => <Self as #crate_path::Operand>::fits_width(v, width),
                    None => true,
                }
            }

            fn can_decode_from_ptr_optional() -> bool {
                <#inner_type as #crate_path::Operand>::can_decode_from_ptr()
            }

            unsafe fn decode_optional_from_ptr(
                width: #crate_path::OperandWidth,
                input: *const u8,
            ) -> (Option<Self>, *const u8) {
                let (val, next) = <#inner_type as #crate_path::Operand>::decode_from_ptr(width, input);
                if val == <#inner_type as Default>::default() {
                    (None, next)
                } else {
                    (Some(Self(val)), next)
                }
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Newtype RelocationField impl
// ---------------------------------------------------------------------------

fn gen_newtype_relocation_field(
    crate_path: &TokenStream2,
    name: &Ident,
    inner_type: &Type,
) -> Result<TokenStream2> {
    Ok(quote! {
        impl #crate_path::RelocationField for #name {
            const TYPE_NAME: &'static str = stringify!(#name);

            fn from_relocation_value(value: i128) -> Result<Self, #crate_path::LinkError> {
                <#inner_type as #crate_path::RelocationField>::from_relocation_value(value).map(Self)
            }

            fn to_relocation_value(&self) -> i128 {
                #crate_path::RelocationField::to_relocation_value(&self.0)
            }
        }
    })
}
