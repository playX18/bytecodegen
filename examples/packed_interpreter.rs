#![allow(incomplete_features)]
#![feature(explicit_tail_calls)]

use bytecodegen::{DisassemblerOptions, Label, LinkedBytecodeView, OperandWidth, bytecode_list};

const REGISTER_COUNT: usize = 4;
const STEP_LIMIT: usize = 64;

bytecode_list! {
    encoding = packed { word_bits = 32 };

    section Bytecode {
        LoadConst {
            args { dst: u8, value: i32 }
            layout {
                word { op: 8, dst: 4, value: 4, dst: 4, value: 12 }
            }
        }

        group BinaryOp = [Add, Sub] {
            args { dst: u8, lhs: u8, rhs: u8 }
            layout {
                word { op: 8, dst: 8, lhs: 8, rhs: 8 }
            }
        }

        JumpIfZero {
            args {
                reg: u8,
                #[relocatable]
                target: i8
            }
            layout {
                word {
                    op: 8,
                    reg: 1, target: 1,
                    reg: 1, target: 1,
                    reg: 1, target: 1,
                    reg: 1, target: 1,
                    reg: 1, target: 1,
                    reg: 1, target: 1,
                    reg: 1, target: 1,
                    reg: 1, target: 1,
                    _: 8
                }
            }
        }

        Jump {
            args {
                #[relocatable]
                target: i8
            }
        }

        LoadWide {
            args { dst: u8, addr: u16 }
            layout {
                word { op: 8, dst: 8, addr: 8, _: 8 }
                word { addr: 8, _: 24 }
            }
        }

        Nop {
            args { count: u8 }
            layout {
                word { op: 8, count: 4, _: 20 }
            }
        }

        Return {
            args { src: u8 }
        }
    }
}

#[derive(Clone)]
struct CodeBlock {
    bytes: Vec<u8>,
}

impl CodeBlock {
    fn len(&self) -> usize {
        self.bytes.len()
    }

    fn pc_to_ip(&self, pc: usize) -> *const u8 {
        assert!(pc <= self.len(), "pc {pc} is out of range");
        unsafe { self.bytes.as_ptr().add(pc) }
    }

    fn pc_to_ip_mut(&mut self, pc: usize) -> *mut u8 {
        assert!(pc <= self.len(), "pc {pc} is out of range");
        unsafe { self.bytes.as_mut_ptr().add(pc) }
    }

    fn ip_to_pc(&self, ip: *const u8) -> usize {
        unsafe { ip.offset_from(self.bytes.as_ptr()) as usize }
    }

    fn end_ip_mut(&mut self) -> *mut u8 {
        unsafe { self.bytes.as_mut_ptr().add(self.len()) }
    }

    fn is_valid_target(&self, target: usize) -> bool {
        target <= self.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    Continue,
    Return(i32),
    Trap(String),
}

#[derive(Debug, Default)]
struct Vm {
    registers: [i32; REGISTER_COUNT],
    trace: Vec<String>,
    steps: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunResult {
    status: Step,
    registers: [i32; REGISTER_COUNT],
    trace: Vec<String>,
    steps: usize,
}

impl Vm {
    fn read(&self, reg: usize) -> Result<i32, String> {
        self.registers
            .get(reg)
            .copied()
            .ok_or_else(|| format!("invalid register r{reg}"))
    }

    fn write(&mut self, reg: usize, value: i32) -> Result<(), String> {
        let slot = self
            .registers
            .get_mut(reg)
            .ok_or_else(|| format!("invalid register r{reg}"))?;
        *slot = value;
        Ok(())
    }

    fn finish(self, status: Step) -> RunResult {
        RunResult {
            status,
            registers: self.registers,
            trace: self.trace,
            steps: self.steps,
        }
    }
}

fn trap(vm: &mut Vm, message: impl Into<String>) -> Step {
    let message = message.into();
    vm.trace.push(format!("trap: {message}"));
    Step::Trap(message)
}

fn exec_invalid(vm: &mut Vm, opcode: u32, width: OperandWidth) -> Step {
    trap(vm, format!("invalid opcode {opcode} decoded at {width:?}"))
}

fn exec_load_const(vm: &mut Vm, dst: u8, value: i32) -> Step {
    if let Err(message) = vm.write(dst as usize, value) {
        return trap(vm, message);
    }
    vm.trace.push(format!("load_const r{dst} <- {value}"));
    Step::Continue
}

fn exec_add(vm: &mut Vm, dst: u8, lhs: u8, rhs: u8) -> Step {
    let lhs_value = match vm.read(lhs as usize) {
        Ok(value) => value,
        Err(message) => return trap(vm, message),
    };
    let rhs_value = match vm.read(rhs as usize) {
        Ok(value) => value,
        Err(message) => return trap(vm, message),
    };
    let sum = match lhs_value.checked_add(rhs_value) {
        Some(value) => value,
        None => {
            return trap(
                vm,
                format!("add overflow: r{lhs}({lhs_value}) + r{rhs}({rhs_value})"),
            );
        }
    };
    if let Err(message) = vm.write(dst as usize, sum) {
        return trap(vm, message);
    }
    vm.trace.push(format!(
        "add r{dst} <- r{lhs}({lhs_value}) + r{rhs}({rhs_value}) = {sum}"
    ));
    Step::Continue
}

fn exec_sub(vm: &mut Vm, dst: u8, lhs: u8, rhs: u8) -> Step {
    let lhs_value = match vm.read(lhs as usize) {
        Ok(value) => value,
        Err(message) => return trap(vm, message),
    };
    let rhs_value = match vm.read(rhs as usize) {
        Ok(value) => value,
        Err(message) => return trap(vm, message),
    };
    let difference = match lhs_value.checked_sub(rhs_value) {
        Some(value) => value,
        None => {
            return trap(
                vm,
                format!("sub overflow: r{lhs}({lhs_value}) - r{rhs}({rhs_value})"),
            );
        }
    };
    if let Err(message) = vm.write(dst as usize, difference) {
        return trap(vm, message);
    }
    vm.trace.push(format!(
        "sub r{dst} <- r{lhs}({lhs_value}) - r{rhs}({rhs_value}) = {difference}"
    ));
    Step::Continue
}

fn exec_return(vm: &mut Vm, src: u8) -> Step {
    let value = match vm.read(src as usize) {
        Ok(value) => value,
        Err(message) => return trap(vm, message),
    };
    vm.trace.push(format!("return r{src}({value})"));
    Step::Return(value)
}

fn resolve_branch_target(
    code: &CodeBlock,
    base: usize,
    delta: i8,
    reason: &str,
) -> Result<usize, String> {
    let target = (base as i64) + i64::from(delta);
    if target < 0 {
        return Err(format!("{reason}: target pc {target} is out of range"));
    }
    let target = target as usize;
    if !code.is_valid_target(target) {
        return Err(format!("{reason}: target pc {target} is out of range"));
    }
    Ok(target)
}

fn branch_to_pc_delta(
    code: &CodeBlock,
    pc: &mut usize,
    vm: &mut Vm,
    delta: i8,
    reason: &str,
) -> Step {
    let target = match resolve_branch_target(code, *pc, delta, reason) {
        Ok(target) => target,
        Err(message) => return trap(vm, message),
    };
    vm.trace.push(format!("{reason} -> pc {target}"));
    *pc = target;
    Step::Continue
}

fn branch_to_ip_delta(
    code: &mut CodeBlock,
    ip: &mut *mut u8,
    vm: &mut Vm,
    delta: i8,
    reason: &str,
) -> Step {
    let base = code.ip_to_pc(*ip);
    let target = match resolve_branch_target(code, base, delta, reason) {
        Ok(target) => target,
        Err(message) => return trap(vm, message),
    };
    vm.trace.push(format!("{reason} -> pc {target}"));
    *ip = code.pc_to_ip_mut(target);
    Step::Continue
}

fn exec_jump_if_zero_pc(code: &CodeBlock, pc: &mut usize, vm: &mut Vm, op: JumpIfZero) -> Step {
    let value = match vm.read(op.reg as usize) {
        Ok(value) => value,
        Err(message) => return trap(vm, message),
    };
    if value == 0 {
        branch_to_pc_delta(
            code,
            pc,
            vm,
            op.target,
            &format!("jump_if_zero r{} == 0", op.reg),
        )
    } else {
        vm.trace
            .push(format!("jump_if_zero r{} == {value}, not taken", op.reg));
        Step::Continue
    }
}

fn exec_jump_if_zero_ip(
    code: &mut CodeBlock,
    ip: &mut *mut u8,
    vm: &mut Vm,
    op: JumpIfZero,
) -> Step {
    let value = match vm.read(op.reg as usize) {
        Ok(value) => value,
        Err(message) => return trap(vm, message),
    };
    if value == 0 {
        branch_to_ip_delta(
            code,
            ip,
            vm,
            op.target,
            &format!("jump_if_zero r{} == 0", op.reg),
        )
    } else {
        vm.trace
            .push(format!("jump_if_zero r{} == {value}, not taken", op.reg));
        Step::Continue
    }
}

fn exec_jump_pc(code: &CodeBlock, pc: &mut usize, vm: &mut Vm, delta: i8) -> Step {
    branch_to_pc_delta(code, pc, vm, delta, "jump")
}

fn exec_jump_ip(code: &mut CodeBlock, ip: &mut *mut u8, vm: &mut Vm, delta: i8) -> Step {
    branch_to_ip_delta(code, ip, vm, delta, "jump")
}

fn exec_load_wide(vm: &mut Vm, dst: u8, addr: u16) -> Step {
    if let Err(message) = vm.write(dst as usize, addr as i32) {
        return trap(vm, message);
    }
    vm.trace.push(format!("load_wide r{dst} <- {addr}"));
    Step::Continue
}

fn exec_nop(vm: &mut Vm, count: u8) -> Step {
    vm.trace.push(format!("nop (count={count})"));
    Step::Continue
}

generated::define_interpreter! {
    fn dispatch_with_pc(code: &CodeBlock, pc: &mut usize, vm: &mut Vm) -> Step;
    ip {
        load = { code.pc_to_ip(*pc) };
        store(next_ip) = { *pc = code.ip_to_pc(next_ip); };
    }
    invalid(opcode, width) => exec_invalid(vm, opcode, width),
    LoadConst(op) => exec_load_const(vm, op.dst, op.value),
    Add(op) => exec_add(vm, op.dst, op.lhs, op.rhs),
    Sub(op) => exec_sub(vm, op.dst, op.lhs, op.rhs),
    JumpIfZero(op) => exec_jump_if_zero_pc(code, pc, vm, op),
    Jump(op) => exec_jump_pc(code, pc, vm, op.target),
    LoadWide(op) => exec_load_wide(vm, op.dst, op.addr),
    Nop(op) => exec_nop(vm, op.count),
    Return(op) => exec_return(vm, op.src),
}

fn prepare_tail_dispatch(code: &mut CodeBlock, ip: *mut u8, vm: &mut Vm) -> Option<Step> {
    if vm.steps == STEP_LIMIT {
        return Some(Step::Trap(format!("step limit of {STEP_LIMIT} reached")));
    }
    if ip == code.end_ip_mut() {
        return Some(Step::Trap("program terminated without Return".to_string()));
    }
    vm.steps += 1;
    None
}

generated::define_tail_interpreter! {
    fn dispatch_with_ip(code: &mut CodeBlock, ip: &mut *mut u8, vm: &mut Vm) -> Step;
    ip {
        load = { *ip };
        store(next_ip) = { *ip = next_ip.cast_mut(); };
    }
    invalid(opcode, width) => exec_invalid(vm, opcode, width),
    LoadConst(op) => match exec_load_const(vm, op.dst, op.value) {
        Step::Continue => {
            if let Some(status) = prepare_tail_dispatch(code, *ip, vm) {
                status
            } else {
                become dispatch_with_ip(code, ip, vm)
            }
        }
        status => status,
    },
    Add(op) => match exec_add(vm, op.dst, op.lhs, op.rhs) {
        Step::Continue => {
            if let Some(status) = prepare_tail_dispatch(code, *ip, vm) {
                status
            } else {
                become dispatch_with_ip(code, ip, vm)
            }
        }
        status => status,
    },
    Sub(op) => match exec_sub(vm, op.dst, op.lhs, op.rhs) {
        Step::Continue => {
            if let Some(status) = prepare_tail_dispatch(code, *ip, vm) {
                status
            } else {
                become dispatch_with_ip(code, ip, vm)
            }
        }
        status => status,
    },
    JumpIfZero(op) => match exec_jump_if_zero_ip(code, ip, vm, op) {
        Step::Continue => {
            if let Some(status) = prepare_tail_dispatch(code, *ip, vm) {
                status
            } else {
                become dispatch_with_ip(code, ip, vm)
            }
        }
        status => status,
    },
    Jump(op) => match exec_jump_ip(code, ip, vm, op.target) {
        Step::Continue => {
            if let Some(status) = prepare_tail_dispatch(code, *ip, vm) {
                status
            } else {
                become dispatch_with_ip(code, ip, vm)
            }
        }
        status => status,
    },
    LoadWide(op) => match exec_load_wide(vm, op.dst, op.addr) {
        Step::Continue => {
            if let Some(status) = prepare_tail_dispatch(code, *ip, vm) {
                status
            } else {
                become dispatch_with_ip(code, ip, vm)
            }
        }
        status => status,
    },
    Nop(op) => match exec_nop(vm, op.count) {
        Step::Continue => {
            if let Some(status) = prepare_tail_dispatch(code, *ip, vm) {
                status
            } else {
                become dispatch_with_ip(code, ip, vm)
            }
        }
        status => status,
    },
    Return(op) => exec_return(vm, op.src),
}

fn pack_linked_program<V: LinkedBytecodeView + ?Sized>(linked: &V) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    for ordinal in 0..linked.instructions().len() {
        let decoded =
            generated::decode_linked_record(linked, ordinal).map_err(|err| err.to_string())?;
        bytes.extend(
            decoded
                .instruction
                .encode_packed_to_bytes()
                .map_err(|err| err.to_string())?,
        );
    }
    Ok(bytes)
}

fn assemble_program() -> (CodeBlock, String) {
    let loop_check = Label::new();
    let done = Label::new();
    let linked = generated::link([
        LoadConst::new(0, 5).into(),
        LoadConst::new(1, 0).into(),
        LoadConst::new(2, 1).into(),
        generated::bind(loop_check),
        JumpIfZero::linkable(0, done),
        Add::new(1, 1, 0).into(),
        Sub::new(0, 0, 2).into(),
        Jump::linkable(loop_check),
        generated::bind(done),
        Return::new(1).into(),
    ])
    .unwrap();
    let packed_bytes = pack_linked_program(&linked).unwrap();
    let disassembly =
        generated::disassemble_packed_bytes(&packed_bytes, &(), &DisassemblerOptions::default())
            .unwrap();
    (
        CodeBlock {
            bytes: linked.bytes().to_vec(),
        },
        disassembly,
    )
}

fn run_with_pc(code: &CodeBlock) -> RunResult {
    let mut pc = 0usize;
    let mut vm = Vm::default();

    loop {
        if vm.steps == STEP_LIMIT {
            return vm.finish(Step::Trap(format!("step limit of {STEP_LIMIT} reached")));
        }
        if pc >= code.len() {
            return vm.finish(Step::Trap("program terminated without Return".to_string()));
        }

        vm.steps += 1;
        match dispatch_with_pc(code, &mut pc, &mut vm) {
            Step::Continue => {}
            status => return vm.finish(status),
        }
    }
}

fn run_with_ip(code: &mut CodeBlock) -> RunResult {
    let mut ip = code.pc_to_ip_mut(0);
    let mut vm = Vm::default();
    let status = match prepare_tail_dispatch(code, ip, &mut vm) {
        Some(status) => status,
        None => dispatch_with_ip(code, &mut ip, &mut vm),
    };
    vm.finish(status)
}

fn main() {
    let (code, disassembly) = assemble_program();
    let pc_result = run_with_pc(&code);
    let ip_result = run_with_ip(&mut code.clone());

    assert_eq!(pc_result, ip_result);
    assert_eq!(pc_result.status, Step::Return(15));

    println!("program bytes: {}", code.len());
    println!("disassembly:");
    println!("{disassembly}");
    println!("status: {:?}", pc_result.status);
    println!("registers: {:?}", pc_result.registers);
    println!("steps: {}", pc_result.steps);
    println!("trace:");
    for entry in &pc_result.trace {
        println!("  {entry}");
    }

    let jz = JumpIfZero::new(7, -3i8);
    let jz_packed = jz.encode_packed_to_bytes().unwrap();
    println!("JumpIfZero packed bytes: {:02x?}", jz_packed);

    let lw = LoadWide::new(2, 0xABCD);
    let lw_packed = lw.encode_packed_to_bytes().unwrap();
    println!("LoadWide packed bytes: {:02x?}", lw_packed);

    // Sub-byte field: count fits in only 4 bits.
    let nop = Nop::new(13);
    let nop_packed = nop.encode_packed_to_bytes().unwrap();
    println!("Nop packed bytes: {:02x?}", nop_packed);
}
