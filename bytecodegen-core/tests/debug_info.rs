use bytecodegen_core::{DebugMap, DebugTable, LinkedDebugInfo};

#[test]
fn debug_table_insert_and_get() {
    let mut table: DebugTable<String> = DebugTable::new();
    let id1 = table.insert("foo".to_string());
    let id2 = table.insert("bar".to_string());
    let id3 = table.insert("foo".to_string()); // duplicate
    assert_eq!(id1, id3, "duplicate insert returns same DebugId");
    assert_ne!(id1, id2);
    assert_eq!(table.get(id1), Some(&"foo".to_string()));
    assert_eq!(table.get(id2), Some(&"bar".to_string()));
    assert_eq!(table.len(), 2);
}

#[test]
fn debug_map_lookup() {
    let mut table: DebugTable<&str> = DebugTable::new();
    let id_a = table.insert("file_a.scm");
    let id_b = table.insert("file_b.scm");
    let map = DebugMap::from_ranges([(0..10, id_a), (10..25, id_b), (25..40, id_a)]);
    assert_eq!(map.at_offset(5), Some(id_a));
    assert_eq!(map.at_offset(10), Some(id_b));
    assert_eq!(map.at_offset(24), Some(id_b));
    assert_eq!(map.at_offset(30), Some(id_a));
    assert_eq!(map.at_offset(50), None);
}

#[test]
fn linked_debug_info_lookup() {
    let mut table: DebugTable<&str> = DebugTable::new();
    let id_foo = table.insert("foo.scm");
    let id_bar = table.insert("bar.scm");
    let map = DebugMap::from_ranges([(0..14, id_foo), (14..28, id_bar)]);
    let info = LinkedDebugInfo::new(table, map);
    assert_eq!(info.at_offset(10), Some(&"foo.scm"));
    assert_eq!(info.at_offset(20), Some(&"bar.scm"));
    assert_eq!(info.at_offset(50), None);
}

#[test]
fn debug_table_len() {
    let mut table: DebugTable<u32> = DebugTable::new();
    assert!(table.is_empty());
    table.insert(1u32);
    assert_eq!(table.len(), 1);
    table.insert(2u32);
    assert_eq!(table.len(), 2);
    table.insert(1u32); // duplicate
    assert_eq!(table.len(), 2);
}
