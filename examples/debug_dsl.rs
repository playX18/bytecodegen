use bytecodegen::core::LinkedProgram;
use bytecodegen::{DebugTable, LinkedBytecodeView, bytecode_list};

bytecode_list! {
    section Vm {
        debug: LineInfo { src: u32, line: u16 }

        LoadConst { args { dst: usize, value: i32 } }
        Return { args { src: usize } }
    }
}

fn main() {
    let mut table = DebugTable::new();
    let id_0 = table.insert(LineInfo { src: 0, line: 0 });
    let id_1 = table.insert(LineInfo { src: 0, line: 5 });

    let load: LinkableInstruction = LoadConst::new(0, 42).into();
    let load = load.with_debug(id_0);
    let ret: LinkableInstruction = Return::new(0).into();
    let ret = ret.with_debug(id_1);

    let result: LinkedProgram<LineInfo> = link(vec![load, ret], table).unwrap();

    assert_eq!(result.bytecode.instruction_count(), 2);

    let info = result
        .debug_info
        .as_ref()
        .expect("debug info should be present");

    let line_info_0 = info
        .at_offset(0)
        .expect("should find debug info at offset 0");
    assert_eq!(line_info_0.src, 0);
    assert_eq!(line_info_0.line, 0);

    let second_offset = result.bytecode.instructions()[1].offset as usize;
    let line_info_1 = info
        .at_offset(second_offset)
        .expect("should find debug info at second instruction");
    assert_eq!(line_info_1.src, 0);
    assert_eq!(line_info_1.line, 5);

    println!(
        "Line info at offset 0: src={}, line={}",
        line_info_0.src, line_info_0.line
    );
    println!(
        "Line info at offset {}: src={}, line={}",
        second_offset, line_info_1.src, line_info_1.line
    );
}
