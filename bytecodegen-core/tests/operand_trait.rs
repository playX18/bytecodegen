use bytecodegen_core::{ByteCursor, Operand, OperandWidth, OptionalOperand};

#[test]
fn u32_fits_narrow() {
    let val: u32 = 200;
    assert!(<u32 as Operand>::fits_width(&val, OperandWidth::Narrow));
}

#[test]
fn u32_does_not_fit_narrow() {
    let val: u32 = 300;
    assert!(!<u32 as Operand>::fits_width(&val, OperandWidth::Narrow));
}

#[test]
fn u32_encode_decode_roundtrip() {
    let val: u32 = 42;
    let mut buf = Vec::new();
    <u32 as Operand>::encode(&val, OperandWidth::Narrow, &mut buf);
    let mut cur = ByteCursor::new(&buf);
    let decoded = <u32 as Operand>::decode(OperandWidth::Narrow, &mut cur).unwrap();
    assert_eq!(decoded, 42);
}

#[test]
fn can_decode_from_ptr_default_is_false() {
    // Primitive types override can_decode_from_ptr to true
    assert!(<u32 as Operand>::can_decode_from_ptr());
}

#[test]
fn i32_encode_decode_roundtrip() {
    let val: i32 = -42;
    let mut buf = Vec::new();
    <i32 as Operand>::encode(&val, OperandWidth::Wide32, &mut buf);
    let mut cur = ByteCursor::new(&buf);
    let decoded = <i32 as Operand>::decode(OperandWidth::Wide32, &mut cur).unwrap();
    assert_eq!(decoded, -42);
}

#[test]
fn bool_encode_decode_roundtrip() {
    let val = true;
    let mut buf = Vec::new();
    <bool as Operand>::encode(&val, OperandWidth::Narrow, &mut buf);
    let mut cur = ByteCursor::new(&buf);
    let decoded: bool = <bool as Operand>::decode(OperandWidth::Narrow, &mut cur).unwrap();
    assert!(decoded);
}

#[test]
fn bool_optional_encode_decode_roundtrip() {
    let val: Option<bool> = Some(true);
    let mut buf = Vec::new();
    <bool as OptionalOperand>::encode_optional(val.as_ref(), OperandWidth::Narrow, &mut buf);
    let mut cur = ByteCursor::new(&buf);
    let decoded =
        <bool as OptionalOperand>::decode_optional(OperandWidth::Narrow, &mut cur).unwrap();
    assert_eq!(decoded, Some(true));

    let none_val: Option<bool> = None;
    let mut buf2 = Vec::new();
    <bool as OptionalOperand>::encode_optional(none_val.as_ref(), OperandWidth::Narrow, &mut buf2);
    let mut cur2 = ByteCursor::new(&buf2);
    let decoded2 =
        <bool as OptionalOperand>::decode_optional(OperandWidth::Narrow, &mut cur2).unwrap();
    assert_eq!(decoded2, None);
}

#[test]
fn encoded_len_matches_width_bytes() {
    assert_eq!(<u32 as Operand>::encoded_len(OperandWidth::Narrow), 1);
    assert_eq!(<u32 as Operand>::encoded_len(OperandWidth::Wide16), 2);
    assert_eq!(<u32 as Operand>::encoded_len(OperandWidth::Wide32), 4);
}
