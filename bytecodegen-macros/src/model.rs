use proc_macro2::TokenStream as TokenStream2;
use quote::format_ident;
use quote::quote;
use syn::{Ident, Type};

use crate::doc::DocBlock;
use crate::names::{is_upper_camel_case, snake_to_upper_camel, upper_camel_to_snake};

#[derive(Clone)]
pub(crate) struct NameModel {
    pub(crate) ident: Ident,
    pub(crate) upper_camel: String,
    pub(crate) snake_case: String,
    pub(crate) screaming_snake_case: String,
    pub(crate) kebab_case: String,
}

impl NameModel {
    pub(crate) fn from_ident(
        ident: Ident,
        require_upper_camel: bool,
        kind: &str,
    ) -> Result<Self, syn::Error> {
        let raw = ident.to_string();
        if require_upper_camel && !is_upper_camel_case(&raw) {
            return Err(syn::Error::new(
                ident.span(),
                format!("{kind} names must use UpperCamelCase"),
            ));
        }

        let snake_case = if require_upper_camel || is_upper_camel_case(&raw) {
            upper_camel_to_snake(&raw)
        } else {
            raw.clone()
        };

        Ok(Self {
            ident,
            upper_camel: raw.clone(),
            screaming_snake_case: snake_case.to_ascii_uppercase(),
            kebab_case: snake_case.replace('_', "-"),
            snake_case,
        })
    }
}

#[derive(Clone)]
pub(crate) struct FieldModel {
    pub(crate) docs: DocBlock,
    pub(crate) ident: Ident,
    pub(crate) optional: bool,
    pub(crate) relocation: Option<RelocationModel>,
    pub(crate) ty: Type,
}

impl FieldModel {
    pub(crate) fn name(&self) -> String {
        self.ident.to_string()
    }

    pub(crate) fn variant_ident(&self) -> Ident {
        format_ident!("{}", snake_to_upper_camel(&self.name()))
    }
}

#[derive(Clone, Copy)]
pub(crate) enum RelocationModel {
    Absolute,
    RelativeNext,
}

impl RelocationModel {
    pub(crate) fn to_core_tokens(self, crate_path: &TokenStream2) -> TokenStream2 {
        match self {
            Self::Absolute => quote!(#crate_path::core::RelocationKind::Absolute),
            Self::RelativeNext => quote!(#crate_path::core::RelocationKind::RelativeNext),
        }
    }
}

#[derive(Clone)]
pub(crate) struct CheckpointModel {
    pub(crate) docs: DocBlock,
    pub(crate) ident: Ident,
}

impl CheckpointModel {
    pub(crate) fn name(&self) -> String {
        self.ident.to_string()
    }

    pub(crate) fn variant_ident(&self) -> Ident {
        format_ident!("{}", snake_to_upper_camel(&self.name()))
    }
}

#[derive(Clone)]
pub(crate) struct BodyModel {
    pub(crate) args: Vec<FieldModel>,
    pub(crate) metadata: Vec<FieldModel>,
    pub(crate) tmps: Vec<FieldModel>,
    pub(crate) checkpoints: Vec<CheckpointModel>,
}

impl BodyModel {
    pub(crate) fn category(&self) -> usize {
        if !self.checkpoints.is_empty() {
            0
        } else if !self.metadata.is_empty() {
            1
        } else {
            2
        }
    }
}

#[derive(Clone)]
pub(crate) struct OpcodeModel {
    pub(crate) docs: DocBlock,
    pub(crate) name: NameModel,
    pub(crate) section: NameModel,
    pub(crate) body: BodyModel,
    pub(crate) verify: Option<VerifyModel>,
}

#[derive(Clone)]
pub(crate) struct VerifyModel {
    pub(crate) kind: VerifyKind,
}

#[derive(Clone)]
pub(crate) enum VerifyKind {
    StackBalance(i32),
    JumpTargetValid,
    #[allow(dead_code)]
    /// TODO: Add more verification kinds via Box<dyn Fn()> or similar once we figure out API for that.
    Custom(()),
}

impl OpcodeModel {
    pub(crate) fn has_relocations(&self) -> bool {
        self.body
            .args
            .iter()
            .any(|field| field.relocation.is_some())
    }
}

#[derive(Clone)]
pub(crate) struct GroupModel {
    pub(crate) docs: DocBlock,
    pub(crate) name: NameModel,
    pub(crate) section: NameModel,
    pub(crate) members: Vec<NameModel>,
    pub(crate) body: BodyModel,
}

pub(crate) struct SectionModel {
    pub(crate) docs: DocBlock,
    pub(crate) name: NameModel,
    pub(crate) preserve_order: bool,
    pub(crate) debug: Option<DebugModel>,
    pub(crate) source_items: Vec<SourceItemModel>,
    pub(crate) opcodes: Vec<OpcodeModel>,
}

#[derive(Clone)]
pub(crate) struct DebugModel {
    pub(crate) name: NameModel,
    pub(crate) fields: Vec<FieldModel>,
}

pub(crate) enum SourceItemModel {
    Opcode(OpcodeModel),
    Group(GroupModel),
}

pub(crate) struct SchemaModel {
    pub(crate) sections: Vec<SectionModel>,
    pub(crate) opcodes: Vec<OpcodeModel>,
    pub(crate) groups: Vec<GroupModel>,
}
