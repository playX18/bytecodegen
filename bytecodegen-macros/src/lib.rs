use proc_macro::TokenStream;
use syn::parse_macro_input;

mod build;
mod derive;
mod doc;
mod doc_gen;
mod group_gen;
mod hash;
mod interpreter_gen;
mod kw;
mod model;
mod names;
mod opcode_gen;
mod packed_gen;
mod parse;
mod schema_gen;

pub(crate) const MAX_NARROW_OPCODE: u32 = u8::MAX as u32 - 2;

#[proc_macro]
pub fn bytecode_list(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as parse::SchemaInput);
    match build::compile_schema(input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[proc_macro_derive(Operand, attributes(operand))]
pub fn derive_operand(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    derive::derive_operand(input).into()
}
