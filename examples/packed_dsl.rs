use bytecodegen::bytecode_list;

bytecode_list! {
    encoding = packed { word_bits = 32 };

    section Bytecode {
        LoadConst {
            args { dst: u8, value: i32 }
        }

        Add {
            args { dst: u8, lhs: u8, rhs: u8 }
            layout {
                word { op: 8, dst: 8, lhs: 8, rhs: 8 }
            }
        }

        JumpIfZero {
            args {
                src: usize,
                #[relocatable]
                target: i32
            }
            layout {
                word {
                    op: 8, _: 1,
                    src: 1, target: 1,
                    src: 1, target: 1,
                    src: 1, target: 1,
                    src: 1, target: 1,
                    src: 1, target: 1,
                    target: 7,
                    _: 6
                }
            }
        }
    }
}

fn main() {
    let mut bytes = Vec::new();
    bytes.extend(
        LoadConst::new(0, i32::MAX)
            .encode_packed_to_bytes()
            .unwrap(),
    );
    bytes.extend(
        LoadConst::new(1, i32::MAX / 2)
            .encode_packed_to_bytes()
            .unwrap(),
    );
    bytes.extend(Add::new(2, 1, 0).encode_packed_to_bytes().unwrap());
    bytes.extend(
        JumpIfZero::new(14, 2042i32)
            .encode_packed_to_bytes()
            .unwrap(),
    );

    println!("{:02x?}", bytes);

    let str = generated::disassemble_packed_bytes(&bytes, &(), &Default::default()).unwrap();
    println!("{}", str);
}
