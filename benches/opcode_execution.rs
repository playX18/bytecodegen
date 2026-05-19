#![allow(incomplete_features)]
#![feature(explicit_tail_calls)]
use std::hint::black_box;

use bytecodegen::{Label, LinkedBytecodeView, OperandWidth, bytecode_list};
use criterion::{Criterion, Throughput, criterion_group, criterion_main};

const REGISTER_COUNT: usize = 4;
const LOOP_COUNT: i32 = 1024 * 1024;

bytecode_list! {
    section Benchcode {
        preserve_order = true;

        LoadConst {
            args {
                dst: u8,
                value: i32,
            }
        }

        Add {
            args {
                dst: u8,
                lhs: u8,
                rhs: u8,
            }
        }

        Sub {
            args {
                dst: u8,
                lhs: u8,
                rhs: u8,
            }
        }

        JumpIfZero {
            args {
                reg: u8,
                #[relocatable]
                target: i32,
            }
        }

        Jump {
            args {
                #[relocatable]
                target: i32,
            }
        }

        Return {
            args {
                src: u8,
            }
        }
    }
}

mod packed {
    use bytecodegen::{Label, LinkedBytecodeView, bytecode_list};

    use super::{
        CodeBlock, LOOP_COUNT, REGISTER_COUNT, branch_target, invalid_opcode, read_register,
    };

    bytecode_list! {
        encoding = packed { word_bits = 32 };

        section PackedBenchcode {
            preserve_order = true;

            LoadConst {
                args {
                    dst: u8,
                    value: i32,
                }
            }

            Add {
                args {
                    dst: u8,
                    lhs: u8,
                    rhs: u8,
                }
            }

            Sub {
                args {
                    dst: u8,
                    lhs: u8,
                    rhs: u8,
                }
            }

            JumpIfZero {
                args {
                    reg: u8,
                    #[relocatable]
                    target: i32,
                }
            }

            Jump {
                args {
                    #[relocatable]
                    target: i32,
                }
            }

            Return {
                args {
                    src: u8,
                }
            }
        }
    }

    pub(super) fn build_encoded_program() -> Vec<u8> {
        let loop_start = Label::new();
        let done = Label::new();
        let linked = generated::link([
            LoadConst::new(0, LOOP_COUNT).into(),
            LoadConst::new(1, 0).into(),
            LoadConst::new(2, 1).into(),
            generated::bind(loop_start),
            JumpIfZero::linkable(0, done),
            Add::new(1, 1, 0).into(),
            Sub::new(0, 0, 2).into(),
            Jump::linkable(loop_start),
            generated::bind(done),
            Return::new(1).into(),
        ])
        .unwrap();
        linked.bytes().to_vec()
    }

    generated::define_interpreter! {
        pub(super) fn dispatch_bytecodegen(code: &CodeBlock, pc: &mut usize, registers: &mut [i32; REGISTER_COUNT]) -> i32;
        ip {
            load = {
                debug_assert!(
                    *pc < code.len(),
                    "packed bytecodegen program terminated without Return"
                );
                code.pc_to_ip(*pc)
            };
            store(next_ip) = { *pc = code.ip_to_pc(next_ip); };
        }
        invalid(opcode, width) => return invalid_opcode(opcode, width).unwrap(),
        LoadConst(op) => {
            registers[usize::from(op.dst)] = i32::from(op.value);
        },
        Add(op) => {
            registers[usize::from(op.dst)] =
                read_register(registers, op.lhs).wrapping_add(read_register(registers, op.rhs));
        },
        Sub(op) => {
            registers[usize::from(op.dst)] =
                read_register(registers, op.lhs).wrapping_sub(read_register(registers, op.rhs));
        },
        JumpIfZero(op) => {
            if read_register(registers, op.reg) == 0 {
                *pc = branch_target(*pc, op.target as isize, code.len());
            }
        },
        Jump(op) => {
            *pc = branch_target(*pc, op.target as isize, code.len());
        },
        Return(op) => return read_register(registers, op.src),
    }

    generated::define_tail_interpreter!(
        pub(super) fn dispatch_bytecodegen_tail(code: &CodeBlock,  pc: usize, registers: &mut [i32; REGISTER_COUNT]) -> i32;
        ip {
            load = { code.pc_to_ip(pc) };
            store(next_ip) = { pc = code.ip_to_pc(next_ip); };
        }
        invalid(opcode, width) => invalid_opcode(opcode, width).unwrap(),
        LoadConst(op) => {
            registers[usize::from(op.dst)] = i32::from(op.value);
            become dispatch_bytecodegen_tail(code, pc, registers)
        },
        Add(op) => {
            registers[usize::from(op.dst)] =
                read_register(registers, op.lhs).wrapping_add(read_register(registers, op.rhs));
            become dispatch_bytecodegen_tail(code, pc, registers)
        },
        Sub(op) => {
            registers[usize::from(op.dst)] =
                read_register(registers, op.lhs).wrapping_sub(read_register(registers, op.rhs));
            become dispatch_bytecodegen_tail(code, pc, registers)
        },
        JumpIfZero(op) => {
            if read_register(registers, op.reg) == 0 {
                pc = branch_target(pc, op.target as isize, code.len());
            }
            become dispatch_bytecodegen_tail(code, pc, registers)
        },
        Jump(op) => {
            pc = branch_target(pc, op.target as isize, code.len());
            become dispatch_bytecodegen_tail(code, pc, registers)
        },
        Return(op) => read_register(registers, op.src),
    );
}

#[derive(Clone, Copy)]
enum EnumOpcode {
    LoadConst { dst: u8, value: i32 },
    Add { dst: u8, lhs: u8, rhs: u8 },
    Sub { dst: u8, lhs: u8, rhs: u8 },
    JumpIfZero { reg: u8, target: i32 },
    Jump { target: i32 },
    Return { src: u8 },
}

fn build_enum_program() -> Vec<EnumOpcode> {
    vec![
        EnumOpcode::LoadConst {
            dst: 0,
            value: LOOP_COUNT,
        },
        EnumOpcode::LoadConst { dst: 1, value: 0 },
        EnumOpcode::LoadConst { dst: 2, value: 1 },
        EnumOpcode::JumpIfZero { reg: 0, target: 3 },
        EnumOpcode::Add {
            dst: 1,
            lhs: 1,
            rhs: 0,
        },
        EnumOpcode::Sub {
            dst: 0,
            lhs: 0,
            rhs: 2,
        },
        EnumOpcode::Jump { target: -4 },
        EnumOpcode::Return { src: 1 },
    ]
}

fn build_encoded_program() -> Vec<u8> {
    let loop_start = Label::new();
    let done = Label::new();
    let linked = generated::link([
        LoadConst::new(0, LOOP_COUNT).into(),
        LoadConst::new(1, 0).into(),
        LoadConst::new(2, 1).into(),
        generated::bind(loop_start),
        JumpIfZero::linkable(0, done),
        Add::new(1, 1, 0).into(),
        Sub::new(0, 0, 2).into(),
        Jump::linkable(loop_start),
        generated::bind(done),
        Return::new(1).into(),
    ])
    .unwrap();
    linked.bytes().to_vec()
}

fn build_packed_encoded_program() -> Vec<u8> {
    packed::build_encoded_program()
}

#[inline(always)]
fn read_register(registers: &[i32; REGISTER_COUNT], reg: u8) -> i32 {
    registers[usize::from(reg)]
}

fn branch_target(pc: usize, target: isize, program_len: usize) -> usize {
    let target = pc
        .checked_add_signed(target)
        .expect("branch target overflowed");
    debug_assert!(
        target < program_len,
        "branch target {target} is out of range"
    );
    target
}

struct CodeBlock {
    bytes: Vec<u8>,
}

impl CodeBlock {
    fn len(&self) -> usize {
        self.bytes.len()
    }

    fn pc_to_ip(&self, pc: usize) -> *const u8 {
        debug_assert!(pc <= self.len(), "pc {pc} is out of range");
        unsafe { self.bytes.as_ptr().add(pc) }
    }

    fn ip_to_pc(&self, ip: *const u8) -> usize {
        unsafe { ip.offset_from(self.bytes.as_ptr()) as usize }
    }
}

fn invalid_opcode(opcode: u32, width: OperandWidth) -> Option<i32> {
    panic!("invalid opcode {opcode} decoded at {width:?}");
}

fn exec_load_const(registers: &mut [i32; REGISTER_COUNT], dst: u8, value: i32) -> Option<i32> {
    registers[usize::from(dst)] = i32::from(value);
    None
}

fn exec_add(registers: &mut [i32; REGISTER_COUNT], dst: u8, lhs: u8, rhs: u8) -> Option<i32> {
    registers[usize::from(dst)] =
        read_register(registers, lhs).wrapping_add(read_register(registers, rhs));
    None
}

fn exec_sub(registers: &mut [i32; REGISTER_COUNT], dst: u8, lhs: u8, rhs: u8) -> Option<i32> {
    registers[usize::from(dst)] =
        read_register(registers, lhs).wrapping_sub(read_register(registers, rhs));
    None
}

fn exec_jump_if_zero(
    code: &CodeBlock,
    pc: &mut usize,
    registers: &[i32; REGISTER_COUNT],
    op: JumpIfZero,
) -> Option<i32> {
    if read_register(registers, op.reg) == 0 {
        *pc = branch_target(*pc, op.target as isize, code.len());
    }
    None
}

fn exec_jump(code: &CodeBlock, pc: &mut usize, target: i32) -> Option<i32> {
    *pc = branch_target(*pc, target as isize, code.len());
    None
}

fn exec_return(registers: &[i32; REGISTER_COUNT], src: u8) -> Option<i32> {
    Some(read_register(registers, src))
}

generated::define_interpreter! {
    fn dispatch_bytecodegen(code: &CodeBlock, pc: &mut usize, registers: &mut [i32; REGISTER_COUNT]) -> i32;
    ip {
        load = {
            debug_assert!(
                *pc < code.len(),
                "bytecodegen program terminated without Return"
            );
            code.pc_to_ip(*pc)
        };
        store(next_ip) = { *pc = code.ip_to_pc(next_ip); };
    }
    invalid(opcode, width) => return invalid_opcode(opcode, width).unwrap(),
    LoadConst(op) => {
        registers[usize::from(op.dst)] = i32::from(op.value);
    },
    Add(op) => {
        registers[usize::from(op.dst)] =
            read_register(registers, op.lhs).wrapping_add(read_register(registers, op.rhs));
    },
    Sub(op) => {
        registers[usize::from(op.dst)] =
            read_register(registers, op.lhs).wrapping_sub(read_register(registers, op.rhs));
    },
    JumpIfZero(op) => {
        if read_register(registers, op.reg) == 0 {
            *pc = branch_target(*pc, op.target as isize, code.len());
        }
    },
    Jump(op) => {
        *pc = branch_target(*pc, op.target as isize, code.len());
    },
    Return(op) => return read_register(registers, op.src),
}

generated::define_tail_interpreter!(
    fn dispatch_bytecodegen_tail(code: &CodeBlock, pc: &mut usize, registers: &mut [i32; REGISTER_COUNT]) -> i32;
    ip {
        load = { code.pc_to_ip(*pc) };
        store(next_ip) = { *pc = code.ip_to_pc(next_ip); };
    }
    invalid(opcode, width) => invalid_opcode(opcode, width).unwrap(),
    LoadConst(op) => match exec_load_const(registers, op.dst, op.value) {
        Some(value) => value,
        None => become dispatch_bytecodegen_tail(code, pc, registers),
    },
    Add(op) => match exec_add(registers, op.dst, op.lhs, op.rhs) {
        Some(value) => value,
        None => become dispatch_bytecodegen_tail(code, pc, registers),
    },
    Sub(op) => match exec_sub(registers, op.dst, op.lhs, op.rhs) {
        Some(value) => value,
        None => become dispatch_bytecodegen_tail(code, pc, registers),
    },
    JumpIfZero(op) => match exec_jump_if_zero(code, pc, registers, op) {
        Some(value) => value,
        None => become dispatch_bytecodegen_tail(code, pc, registers),
    },
    Jump(op) => match exec_jump(code, pc, op.target) {
        Some(value) => value,
        None => become dispatch_bytecodegen_tail(code, pc, registers),
    },
    Return(op) => exec_return(registers, op.src).unwrap(),
);

fn run_enum(program: &[EnumOpcode]) -> i32 {
    let mut registers = [0i32; REGISTER_COUNT];
    let mut pc = 0usize;

    while pc < program.len() {
        let instruction = program[pc];
        pc += 1;

        match instruction {
            EnumOpcode::LoadConst { dst, value } => {
                registers[usize::from(dst)] = i32::from(value);
            }
            EnumOpcode::Add { dst, lhs, rhs } => {
                registers[usize::from(dst)] =
                    read_register(&registers, lhs).wrapping_add(read_register(&registers, rhs));
            }
            EnumOpcode::Sub { dst, lhs, rhs } => {
                registers[usize::from(dst)] =
                    read_register(&registers, lhs).wrapping_sub(read_register(&registers, rhs));
            }
            EnumOpcode::JumpIfZero { reg, target } => {
                if read_register(&registers, reg) == 0 {
                    pc = branch_target(pc, target as _, program.len());
                }
            }
            EnumOpcode::Jump { target } => {
                pc = branch_target(pc, target as _, program.len());
            }
            EnumOpcode::Return { src } => return read_register(&registers, src),
        }
    }

    panic!("enum program terminated without Return")
}

fn run_bytecodegen(code: &CodeBlock) -> i32 {
    let mut registers = [0i32; REGISTER_COUNT];
    let mut pc = 0usize;
    dispatch_bytecodegen(code, &mut pc, &mut registers)
}
fn run_packed_bytecodegen(code: &CodeBlock) -> i32 {
    let mut registers = [0i32; REGISTER_COUNT];
    let mut pc = 0usize;
    packed::dispatch_bytecodegen(code, &mut pc, &mut registers)
}

fn compare_opcode_execution(c: &mut Criterion) {
    let enum_program = build_enum_program();
    let encoded_program = CodeBlock {
        bytes: build_encoded_program(),
    };
    let packed_encoded_program = CodeBlock {
        bytes: build_packed_encoded_program(),
    };
    let expected = run_enum(&enum_program);

    assert_eq!(run_bytecodegen(&encoded_program), expected);
    assert_eq!(run_packed_bytecodegen(&packed_encoded_program), expected);

    let mut group = c.benchmark_group("opcode_execution");
    group.throughput(Throughput::Elements(LOOP_COUNT as u64));
    group.bench_function("bytecodegen_tail", |b| {
        b.iter(|| {
            let mut registers = [0i32; REGISTER_COUNT];
            let mut pc = 0usize;
            black_box(dispatch_bytecodegen_tail(
                black_box(&encoded_program),
                &mut pc,
                &mut registers,
            ))
        });
    });
    group.bench_function("packed_bytecodegen_tail", |b| {
        b.iter(|| {
            let mut registers = [0i32; REGISTER_COUNT];
            let pc = 0usize;
            black_box(packed::dispatch_bytecodegen_tail(
                black_box(&packed_encoded_program),
                pc,
                &mut registers,
            ))
        });
    });
    group.bench_function("rust_enum", |b| {
        b.iter(|| black_box(run_enum(black_box(&enum_program))));
    });
    group.bench_function("bytecodegen_encoded", |b| {
        b.iter(|| black_box(run_bytecodegen(black_box(&encoded_program))));
    });
    group.bench_function("packed_bytecodegen_encoded", |b| {
        b.iter(|| black_box(run_packed_bytecodegen(black_box(&packed_encoded_program))));
    });

    group.finish();
}

criterion_group!(benches, compare_opcode_execution);
criterion_main!(benches);
