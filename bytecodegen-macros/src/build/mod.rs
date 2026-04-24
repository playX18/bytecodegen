mod model;
mod packed;
mod validation;

use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::{Error, Result};

use crate::model::SchemaModel;
use crate::parse::SchemaInput;
use crate::schema_gen::generate_schema;

pub(crate) fn compile_schema(input: SchemaInput) -> Result<TokenStream2> {
    let crate_path = bytecodegen_path()?;
    let schema = SchemaModel::build(input)?;
    Ok(generate_schema(&schema, &crate_path))
}

pub(crate) fn bytecodegen_path() -> Result<TokenStream2> {
    match crate_name("bytecodegen") {
        Ok(FoundCrate::Itself) => Ok(quote!(::bytecodegen)),
        Ok(FoundCrate::Name(name)) => {
            let ident = format_ident!("{name}");
            Ok(quote!(::#ident))
        }
        Err(error) => Err(Error::new(
            Span::call_site(),
            format!("failed to resolve bytecodegen crate: {error}"),
        )),
    }
}
