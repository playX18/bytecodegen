use proc_macro2::Span;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{
    Attribute, Error, Ident, LitBool, LitInt, Result, Token, Type, braced, bracketed,
    punctuated::Punctuated,
};

use crate::doc::{DocBlock, extract_doc_string};
use crate::kw;

pub(crate) struct SchemaInput {
    pub encoding: Option<EncodingDecl>,
    pub sections: Vec<SectionDecl>,
}

impl Parse for SchemaInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let encoding = if input.peek(kw::encoding) {
            Some(input.parse()?)
        } else {
            None
        };
        let mut sections = Vec::new();
        while !input.is_empty() {
            sections.push(input.parse()?);
        }
        Ok(Self { encoding, sections })
    }
}

pub(crate) enum EncodingDecl {
    Packed(PackedEncodingDecl),
}

impl Parse for EncodingDecl {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse::<kw::encoding>()?;
        input.parse::<Token![=]>()?;
        let encoding = if input.peek(kw::packed) {
            Self::Packed(input.parse()?)
        } else {
            return Err(Error::new(
                input.span(),
                "expected `packed` encoding declaration",
            ));
        };
        input.parse::<Token![;]>()?;
        Ok(encoding)
    }
}

pub(crate) struct PackedEncodingDecl {
    pub word_bits_span: Span,
    pub word_bits: u8,
}

impl Parse for PackedEncodingDecl {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse::<kw::packed>()?;
        let content;
        braced!(content in input);

        content.parse::<kw::word_bits>()?;
        content.parse::<Token![=]>()?;
        let word_bits_lit: LitInt = content.parse()?;
        let word_bits_span = word_bits_lit.span();
        let word_bits = word_bits_lit.base10_parse()?;
        if !content.is_empty() {
            return Err(Error::new(
                content.span(),
                "unexpected token in packed encoding declaration",
            ));
        }

        Ok(Self {
            word_bits_span,
            word_bits,
        })
    }
}

pub(crate) struct SectionDecl {
    pub docs: DocBlock,
    pub name: Ident,
    pub preserve_order: bool,
    pub debug: Option<DebugDecl>,
    pub items: Vec<SectionItemDecl>,
}

pub(crate) struct DebugDecl {
    pub name: Ident,
    pub fields: Vec<FieldDecl>,
}

impl Parse for SectionDecl {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let docs = DocBlock::parse_outer(input, "section")?;
        input.parse::<kw::section>()?;
        let name: Ident = input.parse()?;
        let content;
        braced!(content in input);

        let mut preserve_order = false;
        let mut debug = None;
        let mut items = Vec::new();

        while !content.is_empty() {
            let lookahead = content.fork();
            // Peek at outer attributes to distinguish section-level keywords
            // from opcode/group items that may carry #[doc] or #[verify].
            let peeked_attrs = lookahead.call(Attribute::parse_outer)?;
            let has_doc = peeked_attrs.iter().any(|a| a.path().is_ident("doc"));
            let has_verify = peeked_attrs.iter().any(|a| a.path().is_ident("verify"));
            if lookahead.peek(kw::preserve_order) {
                if has_doc || has_verify {
                    return Err(Error::new(
                        content.span(),
                        "doc and verify attributes are not supported on preserve_order settings",
                    ));
                }
                content.parse::<kw::preserve_order>()?;
                content.parse::<Token![=]>()?;
                preserve_order = content.parse::<LitBool>()?.value;
                content.parse::<Token![;]>()?;
                continue;
            }

            if lookahead.peek(kw::debug) {
                if has_doc || has_verify {
                    return Err(Error::new(
                        content.span(),
                        "doc and verify attributes are not supported on debug declarations",
                    ));
                }
                content.parse::<kw::debug>()?;
                content.parse::<Token![:]>()?;
                let name: Ident = content.parse()?;
                let fields = parse_field_block(&content)?;
                debug = Some(DebugDecl { name, fields });
                continue;
            }

            if lookahead.peek(kw::group) {
                items.push(SectionItemDecl::Group(content.parse()?));
            } else {
                items.push(SectionItemDecl::Opcode(content.parse()?));
            }
        }

        Ok(Self {
            docs,
            name,
            preserve_order,
            debug,
            items,
        })
    }
}

pub(crate) enum SectionItemDecl {
    Opcode(OpcodeDecl),
    Group(GroupDecl),
}

pub(crate) struct OpcodeDecl {
    pub docs: DocBlock,
    pub name: Ident,
    pub body: BodyDecl,
    pub verify: Option<VerifyDecl>,
}

impl std::fmt::Debug for OpcodeDecl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpcodeDecl")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl Parse for OpcodeDecl {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let mut docs = Vec::new();
        let mut verify = None;

        for attr in &attrs {
            if attr.path().is_ident("doc") {
                docs.push(extract_doc_string(
                    attr,
                    "only `#[doc = \"...\"]` and `#[verify(...)]` are supported on opcode declarations",
                    "opcode doc attributes must use string literals",
                )?);
                continue;
            }

            if attr.path().is_ident("verify") {
                if verify.is_some() {
                    return Err(Error::new(
                        attr.span(),
                        "duplicate verify attribute on opcode",
                    ));
                }
                verify = Some(parse_verify_attr(attr)?);
                continue;
            }

            return Err(Error::new(
                attr.span(),
                "only `#[doc = \"...\"]` and `#[verify(...)]` are supported on opcode declarations",
            ));
        }

        Ok(Self {
            docs: DocBlock { lines: docs },
            name: input.parse()?,
            body: input.parse()?,
            verify,
        })
    }
}

pub(crate) struct VerifyDecl {
    pub kind: VerifyKind,
}

pub(crate) enum VerifyKind {
    StackBalance(i32),
    JumpTargetValid,
    #[allow(dead_code)]
    Custom(String),
}

fn parse_verify_attr(attr: &Attribute) -> Result<VerifyDecl> {
    match &attr.meta {
        syn::Meta::List(list) => {
            let tokens = &list.tokens;
            let parsed: VerifyKind = syn::parse2(tokens.clone())?;
            Ok(VerifyDecl { kind: parsed })
        }
        _ => Err(Error::new(
            attr.span(),
            "`#[verify(...)]` requires parenthesized arguments",
        )),
    }
}

impl Parse for VerifyKind {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let lookahead = input.lookahead1();
        if lookahead.peek(kw::stack_balance) {
            input.parse::<kw::stack_balance>()?;
            input.parse::<Token![=]>()?;
            let lit: LitInt = input.parse()?;
            let value: i32 = lit.base10_parse()?;
            Ok(VerifyKind::StackBalance(value))
        } else if lookahead.peek(kw::jump_target_valid) {
            input.parse::<kw::jump_target_valid>()?;
            Ok(VerifyKind::JumpTargetValid)
        } else {
            Err(lookahead.error())
        }
    }
}

pub(crate) struct GroupDecl {
    pub docs: DocBlock,
    pub name: Ident,
    pub members: Vec<Ident>,
    pub body: BodyDecl,
}

impl Parse for GroupDecl {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let docs = DocBlock::parse_outer(input, "group")?;
        input.parse::<kw::group>()?;
        let name: Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        let content;
        bracketed!(content in input);
        let members = Punctuated::<Ident, Token![,]>::parse_terminated(&content)?
            .into_iter()
            .collect::<Vec<_>>();
        let body: BodyDecl = input.parse()?;
        Ok(Self {
            docs,
            name,
            members,
            body,
        })
    }
}

#[derive(Clone)]
pub(crate) struct LayoutDecl {
    pub span: Span,
    pub words: Vec<LayoutWordDecl>,
}

impl Parse for LayoutDecl {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let span = input.span();
        let content;
        braced!(content in input);
        let mut words = Vec::new();
        while !content.is_empty() {
            content.parse::<kw::word>()?;
            words.push(content.parse()?);
        }
        Ok(Self { span, words })
    }
}

#[derive(Clone)]
pub(crate) struct LayoutWordDecl {
    pub slices: Vec<LayoutSliceDecl>,
}

impl Parse for LayoutWordDecl {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let content;
        braced!(content in input);
        let mut slices = Vec::new();
        while !content.is_empty() {
            slices.push(content.parse()?);
            if content.peek(Token![,]) {
                content.parse::<Token![,]>()?;
            }
        }
        Ok(Self { slices })
    }
}

#[derive(Clone)]
pub(crate) struct LayoutSliceDecl {
    pub name: Ident,
    pub width_bits: u8,
}

impl Parse for LayoutSliceDecl {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let name = if input.peek(Token![_]) {
            let underscore = input.parse::<Token![_]>()?;
            Ident::new("_", underscore.span)
        } else {
            input.parse()?
        };
        input.parse::<Token![:]>()?;
        let width_bits = input.parse::<LitInt>()?.base10_parse()?;
        Ok(Self { name, width_bits })
    }
}

#[derive(Clone)]
pub(crate) struct BodyDecl {
    pub args: Vec<FieldDecl>,
    pub metadata: Vec<FieldDecl>,
    pub tmps: Vec<FieldDecl>,
    pub checkpoints: Vec<CheckpointDecl>,
    pub layout: Option<LayoutDecl>,
}

impl Parse for BodyDecl {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let content;
        braced!(content in input);

        let mut args = None;
        let mut metadata = None;
        let mut tmps = None;
        let mut checkpoints = None;
        let mut layout = None;

        while !content.is_empty() {
            if content.peek(kw::args) {
                if args.is_some() {
                    return Err(Error::new(content.span(), "duplicate args block"));
                }
                content.parse::<kw::args>()?;
                args = Some(parse_field_block(&content)?);
                continue;
            }

            if content.peek(kw::metadata) {
                if metadata.is_some() {
                    return Err(Error::new(content.span(), "duplicate metadata block"));
                }
                content.parse::<kw::metadata>()?;
                metadata = Some(parse_field_block(&content)?);
                continue;
            }

            if content.peek(kw::tmps) {
                if tmps.is_some() {
                    return Err(Error::new(content.span(), "duplicate tmps block"));
                }
                content.parse::<kw::tmps>()?;
                tmps = Some(parse_field_block(&content)?);
                continue;
            }

            if content.peek(kw::checkpoints) {
                if checkpoints.is_some() {
                    return Err(Error::new(content.span(), "duplicate checkpoints block"));
                }
                content.parse::<kw::checkpoints>()?;
                checkpoints = Some(parse_checkpoint_block(&content)?);
                continue;
            }

            if content.peek(kw::layout) {
                if layout.is_some() {
                    return Err(Error::new(content.span(), "duplicate layout block"));
                }
                content.parse::<kw::layout>()?;
                layout = Some(content.parse()?);
                continue;
            }

            return Err(Error::new(
                content.span(),
                "unexpected token in opcode body",
            ));
        }

        Ok(Self {
            args: args.unwrap_or_default(),
            metadata: metadata.unwrap_or_default(),
            tmps: tmps.unwrap_or_default(),
            checkpoints: checkpoints.unwrap_or_default(),
            layout,
        })
    }
}

pub(crate) fn parse_field_block(input: ParseStream<'_>) -> Result<Vec<FieldDecl>> {
    let content;
    braced!(content in input);
    let mut fields = Vec::new();
    while !content.is_empty() {
        fields.push(content.parse()?);
        if content.peek(Token![,]) {
            content.parse::<Token![,]>()?;
        }
    }
    Ok(fields)
}

pub(crate) fn parse_checkpoint_block(input: ParseStream<'_>) -> Result<Vec<CheckpointDecl>> {
    let content;
    braced!(content in input);
    let mut checkpoints = Vec::new();
    while !content.is_empty() {
        checkpoints.push(content.parse()?);
        if content.peek(Token![,]) {
            content.parse::<Token![,]>()?;
        }
    }
    Ok(checkpoints)
}

#[derive(Clone)]
pub(crate) struct FieldDecl {
    pub docs: DocBlock,
    pub name: Ident,
    pub optional: bool,
    pub relocation: Option<RelocationDecl>,
    pub ty: Type,
}

impl Parse for FieldDecl {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let FieldAttrs { docs, relocation } = parse_field_attrs(input)?;
        let name: Ident = input.parse()?;
        let optional = if input.peek(Token![?]) {
            input.parse::<Token![?]>()?;
            true
        } else {
            false
        };
        input.parse::<Token![:]>()?;
        let ty: Type = input.parse()?;
        Ok(Self {
            docs,
            name,
            optional,
            relocation,
            ty,
        })
    }
}

#[derive(Default)]
pub(crate) struct FieldAttrs {
    pub docs: DocBlock,
    pub relocation: Option<RelocationDecl>,
}

pub(crate) fn parse_field_attrs(input: ParseStream<'_>) -> Result<FieldAttrs> {
    let attrs = input.call(Attribute::parse_outer)?;
    let mut docs = Vec::new();
    let mut relocation = None;

    for attr in attrs {
        if attr.path().is_ident("doc") {
            docs.push(extract_doc_string(
                &attr,
                "only `#[doc = \"...\"]` and `#[relocatable(...)]` are supported on field declarations",
                "field doc attributes must use string literals",
            )?);
            continue;
        }

        if attr.path().is_ident("relocatable") {
            if relocation.is_some() {
                return Err(Error::new(
                    attr.span(),
                    "duplicate relocatable attribute on field",
                ));
            }
            relocation = Some(parse_relocatable_attr(&attr)?);
            continue;
        }

        return Err(Error::new(
            attr.span(),
            "only `#[doc = \"...\"]` and `#[relocatable(...)]` are supported on field declarations",
        ));
    }

    Ok(FieldAttrs {
        docs: DocBlock { lines: docs },
        relocation,
    })
}

pub(crate) fn parse_relocatable_attr(attr: &Attribute) -> Result<RelocationDecl> {
    match &attr.meta {
        syn::Meta::Path(_) => Ok(RelocationDecl::RelativeNext),
        syn::Meta::List(list) => {
            if list.tokens.is_empty() {
                Ok(RelocationDecl::RelativeNext)
            } else {
                syn::parse2::<RelocationDecl>(list.tokens.clone())
            }
        }
        syn::Meta::NameValue(meta) => Err(Error::new(
            meta.value.span(),
            "`#[relocatable(...)]` does not accept name-value syntax",
        )),
    }
}

#[derive(Clone, Copy)]
pub(crate) enum RelocationDecl {
    Absolute,
    RelativeNext,
}

impl Parse for RelocationDecl {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        if input.peek(kw::absolute) {
            input.parse::<kw::absolute>()?;
            Ok(Self::Absolute)
        } else if input.peek(kw::relative_next) {
            input.parse::<kw::relative_next>()?;
            Ok(Self::RelativeNext)
        } else {
            Err(Error::new(
                input.span(),
                "expected `absolute` or `relative_next` in relocation annotation",
            ))
        }
    }
}

#[derive(Clone)]
pub(crate) struct CheckpointDecl {
    pub docs: DocBlock,
    pub name: Ident,
}

impl Parse for CheckpointDecl {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let docs = DocBlock::parse_outer(input, "checkpoint")?;
        Ok(Self {
            docs,
            name: input.parse()?,
        })
    }
}
