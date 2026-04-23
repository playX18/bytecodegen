use bytecodegen_core::{
    LinkAllocations, LinkBackendV2, LinkSpec, LinkedInstructionEntry, Placement,
};
use std::alloc::Layout;

#[test]
fn link_spec_has_fields() {
    let spec = LinkSpec {
        code_size: 100,
        instruction_count: 10,
        metadata_layout: Layout::new::<u8>(),
    };
    assert_eq!(spec.code_size, 100);
    assert_eq!(spec.instruction_count, 10);
}
