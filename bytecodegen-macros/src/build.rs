use std::collections::{HashMap, HashSet};

use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{Error, GenericArgument, Ident, Result, Type};

use crate::doc::DocBlock;
use crate::model::{
    BodyModel, CheckpointModel, DebugModel, FieldModel, GroupModel, NameModel, OpcodeModel,
    RelocationModel, SchemaModel, SectionModel, SourceItemModel, VerifyKind as VerifyKindModel,
    VerifyModel,
};
use crate::parse::{
    BodyDecl, CheckpointDecl, DebugDecl, FieldDecl, GroupDecl, OpcodeDecl, RelocationDecl,
    SchemaInput, SectionItemDecl, VerifyKind as VerifyKindDecl,
};
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

impl SchemaModel {
    pub(crate) fn build(input: SchemaInput) -> Result<Self> {
        let mut section_name_map = HashMap::<String, Span>::new();
        let mut opcode_name_map = HashMap::<String, Span>::new();
        let mut group_name_map = HashMap::<String, Span>::new();
        let mut opcode_and_group_map = HashMap::<String, (&'static str, Span)>::new();

        let mut sections = Vec::new();
        let mut all_opcodes = Vec::new();
        let mut all_groups = Vec::new();

        for section_decl in input.sections {
            let section_name = NameModel::from_ident(section_decl.name, false, "section")?;
            ensure_unique(
                &mut section_name_map,
                section_name.snake_case.clone(),
                section_name.ident.span(),
                "duplicate section name after normalization",
            )?;

            let mut source_items = Vec::new();
            let mut opcodes_in_source_order = Vec::new();

            for item in section_decl.items {
                match item {
                    SectionItemDecl::Opcode(opcode_decl) => {
                        let opcode = build_opcode(opcode_decl, &section_name)?;
                        ensure_unique(
                            &mut opcode_name_map,
                            opcode.name.snake_case.clone(),
                            opcode.name.ident.span(),
                            "duplicate opcode name after normalization",
                        )?;
                        ensure_cross_unique(
                            &mut opcode_and_group_map,
                            opcode.name.snake_case.clone(),
                            opcode.name.ident.span(),
                            "opcode",
                        )?;
                        opcodes_in_source_order.push(opcode.clone());
                        source_items.push(SourceItemModel::Opcode(opcode));
                    }
                    SectionItemDecl::Group(group_decl) => {
                        let group = build_group(group_decl, &section_name)?;
                        ensure_unique(
                            &mut group_name_map,
                            group.name.snake_case.clone(),
                            group.name.ident.span(),
                            "duplicate group name after normalization",
                        )?;
                        ensure_cross_unique(
                            &mut opcode_and_group_map,
                            group.name.snake_case.clone(),
                            group.name.ident.span(),
                            "group",
                        )?;
                        for member in &group.members {
                            ensure_unique(
                                &mut opcode_name_map,
                                member.snake_case.clone(),
                                member.ident.span(),
                                "duplicate opcode name after normalization",
                            )?;
                            ensure_cross_unique(
                                &mut opcode_and_group_map,
                                member.snake_case.clone(),
                                member.ident.span(),
                                "opcode",
                            )?;
                            opcodes_in_source_order.push(OpcodeModel {
                                docs: DocBlock::default(),
                                name: member.clone(),
                                section: section_name.clone(),
                                body: group.body.clone(),
                                verify: None,
                            });
                        }
                        all_groups.push(group.clone());
                        source_items.push(SourceItemModel::Group(group));
                    }
                }
            }

            if section_decl.preserve_order {
                validate_opcode_order(&opcodes_in_source_order)?;
            } else {
                stable_sort_opcodes(&mut opcodes_in_source_order);
            }

            all_opcodes.extend(opcodes_in_source_order.iter().cloned());

            let debug = match section_decl.debug {
                Some(debug_decl) => Some(build_debug(debug_decl)?),
                None => None,
            };

            sections.push(SectionModel {
                docs: section_decl.docs,
                name: section_name,
                preserve_order: section_decl.preserve_order,
                debug,
                source_items,
                opcodes: opcodes_in_source_order,
            });
        }

        Ok(Self {
            sections,
            opcodes: all_opcodes,
            groups: all_groups,
        })
    }
}

pub(crate) fn build_opcode(
    opcode_decl: OpcodeDecl,
    section_name: &NameModel,
) -> Result<OpcodeModel> {
    let name = NameModel::from_ident(opcode_decl.name, true, "opcode")?;
    let body = build_body(opcode_decl.body)?;
    validate_opcode_semantics(&name, &body)?;
    let verify = opcode_decl.verify.map(|v| VerifyModel {
        kind: match v.kind {
            VerifyKindDecl::StackBalance(n) => VerifyKindModel::StackBalance(n),
            VerifyKindDecl::JumpTargetValid => VerifyKindModel::JumpTargetValid,
            VerifyKindDecl::Custom(_) => VerifyKindModel::Custom(()),
        },
    });
    Ok(OpcodeModel {
        docs: opcode_decl.docs,
        name,
        section: section_name.clone(),
        body,
        verify,
    })
}

pub(crate) fn build_group(group_decl: GroupDecl, section_name: &NameModel) -> Result<GroupModel> {
    let name = NameModel::from_ident(group_decl.name, true, "group")?;
    if group_decl.members.is_empty() {
        return Err(Error::new(
            name.ident.span(),
            "group declarations must contain at least one opcode member",
        ));
    }

    let mut members = Vec::new();
    let mut seen_members = HashSet::new();
    for ident in group_decl.members {
        let member = NameModel::from_ident(ident, true, "group member")?;
        if !seen_members.insert(member.snake_case.clone()) {
            return Err(Error::new(
                member.ident.span(),
                "group members must be unique after normalization",
            ));
        }
        members.push(member);
    }

    let body = build_body(group_decl.body)?;
    validate_opcode_semantics(&name, &body)?;

    Ok(GroupModel {
        docs: group_decl.docs,
        name,
        section: section_name.clone(),
        members,
        body,
    })
}

pub(crate) fn build_debug(debug_decl: DebugDecl) -> Result<DebugModel> {
    let name = NameModel::from_ident(debug_decl.name, true, "debug type")?;
    let fields = build_fields(debug_decl.fields, "debug field", false)?;
    Ok(DebugModel { name, fields })
}

pub(crate) fn build_body(body_decl: BodyDecl) -> Result<BodyModel> {
    Ok(BodyModel {
        args: build_fields(body_decl.args, "argument", true)?,
        metadata: build_fields(body_decl.metadata, "metadata field", false)?,
        tmps: build_fields(body_decl.tmps, "tmp field", false)?,
        checkpoints: build_checkpoints(body_decl.checkpoints)?,
    })
}

pub(crate) fn build_fields(
    fields: Vec<FieldDecl>,
    kind: &str,
    allow_relocation: bool,
) -> Result<Vec<FieldModel>> {
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    for field in fields {
        validate_snake_case_name(&field.name, kind)?;
        validate_supported_type(&field.ty)?;
        if field.relocation.is_some() && !allow_relocation {
            return Err(Error::new(
                field.name.span(),
                "relocations are only supported on args fields",
            ));
        }
        if field.optional && field.relocation.is_some() {
            return Err(Error::new(
                field.name.span(),
                "optional operands cannot be relocatable",
            ));
        }
        let name = field.name.to_string();
        if !seen.insert(name.clone()) {
            return Err(Error::new(
                field.name.span(),
                format!("duplicate {kind} name `{name}`"),
            ));
        }
        output.push(FieldModel {
            docs: field.docs,
            ident: field.name,
            optional: field.optional,
            relocation: field.relocation.map(|relocation| match relocation {
                RelocationDecl::Absolute => RelocationModel::Absolute,
                RelocationDecl::RelativeNext => RelocationModel::RelativeNext,
            }),
            ty: field.ty,
        });
    }
    Ok(output)
}

pub(crate) fn build_checkpoints(checkpoints: Vec<CheckpointDecl>) -> Result<Vec<CheckpointModel>> {
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    for checkpoint in checkpoints {
        validate_snake_case_name(&checkpoint.name, "checkpoint")?;
        let name = checkpoint.name.to_string();
        if !seen.insert(name.clone()) {
            return Err(Error::new(
                checkpoint.name.span(),
                format!("duplicate checkpoint name `{name}`"),
            ));
        }
        output.push(CheckpointModel {
            docs: checkpoint.docs,
            ident: checkpoint.name,
        });
    }
    Ok(output)
}

pub(crate) fn validate_opcode_semantics(name: &NameModel, body: &BodyModel) -> Result<()> {
    if !body.checkpoints.is_empty() && body.metadata.is_empty() {
        return Err(Error::new(
            name.ident.span(),
            "checkpoints require a metadata block",
        ));
    }
    Ok(())
}

pub(crate) fn validate_opcode_order(opcodes: &[OpcodeModel]) -> Result<()> {
    let mut last_category = 0usize;
    let mut last_name = None::<String>;

    for opcode in opcodes {
        let category = opcode.body.category();
        if last_name.is_none() {
            last_category = category;
            last_name = Some(opcode.name.upper_camel.clone());
            continue;
        }

        if category < last_category {
            let previous = last_name.unwrap_or_default();
            return Err(Error::new(
                opcode.name.ident.span(),
                format!(
                    "invalid preserve_order layout: `{}` appears after `{previous}` but belongs to an earlier opcode category",
                    opcode.name.upper_camel
                ),
            ));
        }

        last_category = category;
        last_name = Some(opcode.name.upper_camel.clone());
    }

    Ok(())
}

pub(crate) fn stable_sort_opcodes(opcodes: &mut [OpcodeModel]) {
    opcodes.sort_by_key(|opcode| opcode.body.category());
}

pub(crate) fn ensure_unique(
    map: &mut HashMap<String, Span>,
    key: String,
    span: Span,
    message: &str,
) -> Result<()> {
    if let Some(previous_span) = map.insert(key.clone(), span) {
        let mut error = Error::new(span, format!("{message}: `{key}`"));
        error.combine(Error::new(previous_span, "previous declaration is here"));
        return Err(error);
    }
    Ok(())
}

pub(crate) fn ensure_cross_unique(
    map: &mut HashMap<String, (&'static str, Span)>,
    key: String,
    span: Span,
    kind: &'static str,
) -> Result<()> {
    if let Some((existing_kind, previous_span)) = map.insert(key.clone(), (kind, span)) {
        let mut error = Error::new(
            span,
            format!("duplicate logical name `{key}` between {existing_kind} and {kind}"),
        );
        error.combine(Error::new(previous_span, "previous declaration is here"));
        return Err(error);
    }
    Ok(())
}

pub(crate) fn validate_snake_case_name(ident: &Ident, kind: &str) -> Result<()> {
    let name = ident.to_string();
    let valid = name == "_"
        || name.chars().enumerate().all(|(index, ch)| match ch {
            'a'..='z' | '0'..='9' | '_' => !(index == 0 && ch.is_ascii_digit()),
            _ => false,
        });
    if !valid || name.contains("__") {
        return Err(Error::new(
            ident.span(),
            format!("{kind} names must use snake_case"),
        ));
    }
    Ok(())
}

pub(crate) fn validate_supported_type(ty: &Type) -> Result<()> {
    match ty {
        Type::Path(path) => {
            for segment in &path.path.segments {
                match &segment.arguments {
                    syn::PathArguments::None => {}
                    syn::PathArguments::AngleBracketed(arguments) => {
                        for argument in &arguments.args {
                            match argument {
                                GenericArgument::Type(ty) => validate_supported_type(ty)?,
                                GenericArgument::Const(_) => {}
                                GenericArgument::AssocType(binding) => {
                                    validate_supported_type(&binding.ty)?
                                }
                                GenericArgument::AssocConst(_) => {}
                                GenericArgument::Constraint(_) => {
                                    return Err(Error::new(
                                        argument.span(),
                                        "constraint generic arguments are not supported in stored operand types",
                                    ));
                                }
                                GenericArgument::Lifetime(_) => {
                                    return Err(Error::new(
                                        argument.span(),
                                        "lifetime-bearing operand types are not supported",
                                    ));
                                }
                                _ => {}
                            }
                        }
                    }
                    syn::PathArguments::Parenthesized(arguments) => {
                        return Err(Error::new(
                            arguments.span(),
                            "function-style generic arguments are not supported in stored operand types",
                        ));
                    }
                }
            }
            Ok(())
        }
        Type::Ptr(pointer) => validate_supported_type(&pointer.elem),
        Type::Array(array) => validate_supported_type(&array.elem),
        Type::Slice(slice) => validate_supported_type(&slice.elem),
        Type::Tuple(tuple) => {
            for elem in &tuple.elems {
                validate_supported_type(elem)?;
            }
            Ok(())
        }
        Type::Paren(paren) => validate_supported_type(&paren.elem),
        Type::Group(group) => validate_supported_type(&group.elem),
        Type::Reference(reference) => Err(Error::new(
            reference.span(),
            "reference operand types are not supported; store by value instead",
        )),
        Type::BareFn(ty) => Err(Error::new(
            ty.span(),
            "function pointer operand types are not supported",
        )),
        Type::ImplTrait(ty) => Err(Error::new(
            ty.span(),
            "impl Trait operand types are not supported",
        )),
        Type::TraitObject(ty) => Err(Error::new(
            ty.span(),
            "trait object operand types are not supported",
        )),
        Type::Infer(ty) => Err(Error::new(
            ty.span(),
            "inferred operand types are not supported",
        )),
        Type::Macro(ty) => Err(Error::new(
            ty.span(),
            "macro operand types are not supported",
        )),
        Type::Never(ty) => Err(Error::new(
            ty.span(),
            "never operand types are not supported",
        )),
        _ => Err(Error::new(
            ty.span(),
            "this operand type shape is not supported",
        )),
    }
}
