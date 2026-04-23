use proc_macro2::TokenStream as TokenStream2;
use quote::format_ident;
use quote::quote;

use crate::doc_gen::{combine_doc_attrs, group_doc_lines, group_predicate_doc_lines};
use crate::model::{GroupModel, SectionModel, SourceItemModel};

pub(crate) fn generate_group_items(group: &GroupModel, _crate_path: &TokenStream2) -> TokenStream2 {
    let members_ident = format_ident!("{}_GROUP_MEMBERS", group.name.screaming_snake_case);
    let predicate_ident = format_ident!("is_{}", group.name.snake_case);
    let members_len = group.members.len();
    let opcode_ids = group.members.iter().map(|member| {
        let ident = &member.ident;
        quote!(OpcodeId::#ident,)
    });
    let match_members = group.members.iter().map(|member| {
        let ident = &member.ident;
        quote!(OpcodeId::#ident)
    });
    let group_docs = combine_doc_attrs(&group.docs, group_doc_lines(group));
    let predicate_docs = combine_doc_attrs(&group.docs, group_predicate_doc_lines(group));

    quote! {
        #group_docs
        pub static #members_ident: [OpcodeId; #members_len] = [#(#opcode_ids)*];

        #predicate_docs
        pub const fn #predicate_ident(opcode: OpcodeId) -> bool {
            matches!(opcode, #(#match_members)|*)
        }
    }
}

pub(crate) fn generate_section_items(
    section: &SectionModel,
    _crate_path: &TokenStream2,
) -> TokenStream2 {
    let opcodes_ident = format_ident!("{}_SECTION_OPCODES", section.name.screaming_snake_case);
    let items_ident = format_ident!("{}_SECTION_ITEMS", section.name.screaming_snake_case);
    let predicate_ident = format_ident!("is_in_{}", section.name.snake_case);
    let opcodes_len = section.opcodes.len();
    let items_len = section.source_items.len();
    let opcode_ids = section.opcodes.iter().map(|opcode| {
        let ident = &opcode.name.ident;
        quote!(OpcodeId::#ident,)
    });
    let match_members = section.opcodes.iter().map(|opcode| {
        let ident = &opcode.name.ident;
        quote!(OpcodeId::#ident)
    });
    let source_items = section.source_items.iter().map(|item| match item {
        SourceItemModel::Opcode(opcode) => {
            let ident = &opcode.name.ident;
            quote!(SectionItem::Opcode(OpcodeId::#ident),)
        }
        SourceItemModel::Group(group) => {
            let name = &group.name.snake_case;
            let members_ident = format_ident!("{}_GROUP_MEMBERS", group.name.screaming_snake_case);
            quote!(SectionItem::Group { name: #name, members: &#members_ident },)
        }
    });
    let opcode_docs = combine_doc_attrs(
        &section.docs,
        vec![format!(
            "Opcode ids that belong to section `{}`.",
            section.name.upper_camel
        )],
    );
    let source_items_docs = combine_doc_attrs(
        &section.docs,
        vec![format!(
            "Top-level source items declared in section `{}`.",
            section.name.upper_camel
        )],
    );
    let predicate_docs = combine_doc_attrs(
        &section.docs,
        vec![format!(
            "Returns whether `opcode` belongs to section `{}`.",
            section.name.upper_camel
        )],
    );
    let predicate_body = if section.opcodes.is_empty() {
        quote!(false)
    } else {
        quote!(matches!(opcode, #(#match_members)|*))
    };

    quote! {
        #opcode_docs
        pub static #opcodes_ident: [OpcodeId; #opcodes_len] = [#(#opcode_ids)*];

        #source_items_docs
        pub static #items_ident: [SectionItem; #items_len] = [#(#source_items)*];

        #predicate_docs
        pub const fn #predicate_ident(opcode: OpcodeId) -> bool {
            #predicate_body
        }
    }
}
