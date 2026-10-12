use crate::doc_gen::render_type_name;
use crate::model::{BodyModel, SchemaModel, SourceItemModel};

pub(crate) struct StableHasher {
    state: u64,
}

impl StableHasher {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    pub(crate) fn new() -> Self {
        Self {
            state: Self::FNV_OFFSET,
        }
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(Self::FNV_PRIME);
        }
    }

    fn write_str(&mut self, value: &str) {
        self.write_bytes(value.as_bytes());
        self.write_bytes(&[0xff]);
    }

    fn write_bool(&mut self, value: bool) {
        self.write_bytes(&[u8::from(value)]);
    }

    fn finish(self) -> u64 {
        self.state
    }
}

pub(crate) fn hash_body(hasher: &mut StableHasher, body: &BodyModel) {
    for field in &body.args {
        hasher.write_str("arg");
        hasher.write_str(&field.name());
        hasher.write_bool(field.optional);
        hasher.write_str(match field.relocation {
            Some(crate::model::RelocationModel::Absolute) => "absolute",
            Some(crate::model::RelocationModel::RelativeNext) => "relative_next",
            Some(crate::model::RelocationModel::RelativeStart) => "relative_start",
            None => "none",
        });
        hasher.write_str(&render_type_name(&field.ty));
    }

    for field in &body.metadata {
        hasher.write_str("metadata");
        hasher.write_str(&field.name());
        hasher.write_str(&render_type_name(&field.ty));
    }

    for field in &body.tmps {
        hasher.write_str("tmp");
        hasher.write_str(&field.name());
        hasher.write_str(&render_type_name(&field.ty));
    }

    for checkpoint in &body.checkpoints {
        hasher.write_str("checkpoint");
        hasher.write_str(&checkpoint.name());
    }
}

pub(crate) fn schema_macro_suffix(schema: &SchemaModel) -> String {
    let mut hasher = StableHasher::new();

    for section in &schema.sections {
        hasher.write_str("section");
        hasher.write_str(&section.name.snake_case);
        hasher.write_bool(section.preserve_order);

        for item in &section.source_items {
            match item {
                SourceItemModel::Opcode(opcode) => {
                    hasher.write_str("opcode");
                    hasher.write_str(&opcode.name.snake_case);
                    hasher.write_str(&opcode.section.snake_case);
                    hash_body(&mut hasher, &opcode.body);
                }
                SourceItemModel::Group(group) => {
                    hasher.write_str("group");
                    hasher.write_str(&group.name.snake_case);
                    hasher.write_str(&group.section.snake_case);
                    for member in &group.members {
                        hasher.write_str(&member.snake_case);
                    }
                    hash_body(&mut hasher, &group.body);
                }
            }
        }
    }

    format!("{:016x}", hasher.finish())
}
