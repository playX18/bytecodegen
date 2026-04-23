use syn::parse::ParseStream;
use syn::spanned::Spanned;
use syn::{Attribute, Error, Result};

#[derive(Clone, Default)]
pub(crate) struct DocBlock {
    pub lines: Vec<String>,
}

impl DocBlock {
    pub(crate) fn parse_outer(input: ParseStream<'_>, target: &str) -> Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        Self::from_attrs(attrs, target)
    }

    pub(crate) fn from_attrs(attrs: Vec<Attribute>, target: &str) -> Result<Self> {
        let mut lines = Vec::new();
        for attr in attrs {
            if !attr.path().is_ident("doc") {
                return Err(Error::new(
                    attr.span(),
                    format!("only doc attributes are supported on {target} declarations"),
                ));
            }
            lines.push(extract_doc_string(
                &attr,
                &format!(
                    "only `#[doc = \"...\"]` attributes are supported on {target} declarations"
                ),
                &format!("{target} doc attributes must use string literals"),
            )?);
        }

        Ok(Self { lines })
    }
}

pub(crate) fn extract_doc_string(
    attr: &Attribute,
    bad_shape: &str,
    bad_value: &str,
) -> Result<String> {
    use syn::{Expr, ExprLit, Lit};

    match &attr.meta {
        syn::Meta::NameValue(meta) => match &meta.value {
            Expr::Lit(ExprLit {
                lit: Lit::Str(value),
                ..
            }) => Ok(value.value()),
            _ => Err(Error::new(meta.value.span(), bad_value)),
        },
        _ => Err(Error::new(attr.span(), bad_shape)),
    }
}
