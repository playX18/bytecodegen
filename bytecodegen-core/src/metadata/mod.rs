mod backend;
mod id;
mod planning;
mod views;

pub use backend::{
    LinkAllocations, LinkBackendV2, LinkSpec, LinkedArtifactSpec, NativeHeapBackend,
    NativeLinkedBytecode, Placement,
};
pub use id::MetadataId;
pub use planning::{
    MetadataHeaderEntry, MetadataTableLayout, MetadataTypeSpec, initialize_metadata_table,
    metadata_slot_ptr, plan_metadata_table,
};
pub use views::{
    LinkedBytecodeView, LinkedInstructionEntry, MetadataTableView, MetadataTableViewMut,
};
