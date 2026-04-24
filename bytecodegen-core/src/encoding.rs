use std::ptr::NonNull;

use crate::byte_io::{
    ByteCursor, ByteSink, assert_non_null_ptr_decode, assert_signed_ptr_decode_fits,
    assert_unsigned_ptr_decode_fits, assert_valid_bool_ptr_decode, read_ptr_u8, read_ptr_u16,
    read_ptr_u32,
};
use crate::error::{DecodeError, LinkError};
use crate::traits::{Operand, OptionalOperand};
use crate::width::{OperandWidth, RelocationField};

fn encode_unsigned<S: ByteSink + ?Sized>(raw: u32, width: OperandWidth, output: &mut S) {
    match width {
        OperandWidth::Narrow => output.push(raw as u8),
        OperandWidth::Wide16 => output.extend_from_slice(&(raw as u16).to_le_bytes()),
        OperandWidth::Wide32 => output.extend_from_slice(&raw.to_le_bytes()),
    }
}

fn encode_signed<S: ByteSink + ?Sized>(raw: i32, width: OperandWidth, output: &mut S) {
    encode_unsigned(raw as u32, width, output)
}

macro_rules! impl_unsigned_encoding {
    ($($ty:ty),* $(,)?) => {
        $(
            impl Operand for $ty {
                fn fits_width(value: &Self, width: OperandWidth) -> bool {
                    let value = *value as u128;
                    match width {
                        OperandWidth::Narrow => value <= u8::MAX as u128,
                        OperandWidth::Wide16 => value <= u16::MAX as u128,
                        OperandWidth::Wide32 => value <= u32::MAX as u128,
                    }
                }

                fn encode<S: ByteSink + ?Sized>(value: &Self, width: OperandWidth, output: &mut S) {
                    encode_unsigned(*value as u32, width, output)
                }

                fn decode(width: OperandWidth, input: &mut ByteCursor<'_>) -> Result<Self, DecodeError> {
                    let raw = input.read_unsigned(width)?;
                    <$ty>::try_from(raw).map_err(|_| DecodeError::InvalidValue("unsigned value does not fit target type"))
                }

                fn encoded_len(width: OperandWidth) -> usize {
                    width.bytes()
                }

                fn can_decode_from_ptr() -> bool { true }

                unsafe fn decode_from_ptr(width: OperandWidth, input: *const u8) -> (Self, *const u8) {
                    let (raw, next) = match width {
                        OperandWidth::Narrow => (u32::from(unsafe { read_ptr_u8(input) }), unsafe {
                            input.add(1)
                        }),
                        OperandWidth::Wide16 => (
                            u32::from(unsafe { read_ptr_u16(input) }),
                            unsafe { input.add(2) },
                        ),
                        OperandWidth::Wide32 => (
                            unsafe { read_ptr_u32(input) },
                            unsafe { input.add(4) },
                        ),
                    };
                    #[cfg(debug_assertions)]
                    assert_unsigned_ptr_decode_fits::<$ty>(raw, stringify!($ty));
                    (raw as $ty, next)
                }
            }

            impl RelocationField for $ty {
                const TYPE_NAME: &'static str = stringify!($ty);

                fn from_relocation_value(value: i128) -> Result<Self, LinkError> {
                    <$ty>::try_from(value).map_err(|_| LinkError::RelocationValueOutOfRange {
                        type_name: Self::TYPE_NAME,
                        value,
                    })
                }

                fn to_relocation_value(&self) -> i128 {
                    *self as i128
                }
            }
        )*
    };
}

macro_rules! impl_signed_encoding {
    ($($ty:ty),* $(,)?) => {
        $(
            impl Operand for $ty {
                fn fits_width(value: &Self, width: OperandWidth) -> bool {
                    let value = *value as i128;
                    match width {
                        OperandWidth::Narrow => (i8::MIN as i128..=i8::MAX as i128).contains(&value),
                        OperandWidth::Wide16 => (i16::MIN as i128..=i16::MAX as i128).contains(&value),
                        OperandWidth::Wide32 => (i32::MIN as i128..=i32::MAX as i128).contains(&value),
                    }
                }

                fn encode<S: ByteSink + ?Sized>(value: &Self, width: OperandWidth, output: &mut S) {
                    encode_signed(*value as i32, width, output)
                }

                fn decode(width: OperandWidth, input: &mut ByteCursor<'_>) -> Result<Self, DecodeError> {
                    let raw = input.read_signed(width)?;
                    <$ty>::try_from(raw).map_err(|_| DecodeError::InvalidValue("signed value does not fit target type"))
                }

                fn encoded_len(width: OperandWidth) -> usize {
                    width.bytes()
                }

                fn can_decode_from_ptr() -> bool { true }

                unsafe fn decode_from_ptr(width: OperandWidth, input: *const u8) -> (Self, *const u8) {
                    let (raw, next) = match width {
                        OperandWidth::Narrow => (
                            i32::from(i8::from_le_bytes([unsafe { read_ptr_u8(input) }])),
                            unsafe { input.add(1) },
                        ),
                        OperandWidth::Wide16 => (
                            i32::from(i16::from_le_bytes(unsafe {
                                read_ptr_u16(input).to_le_bytes()
                            })),
                            unsafe { input.add(2) },
                        ),
                        OperandWidth::Wide32 => (
                            i32::from_le_bytes(unsafe { read_ptr_u32(input).to_le_bytes() }),
                            unsafe { input.add(4) },
                        ),
                    };
                    #[cfg(debug_assertions)]
                    assert_signed_ptr_decode_fits::<$ty>(raw, stringify!($ty));
                    (raw as $ty, next)
                }
            }

            impl RelocationField for $ty {
                const TYPE_NAME: &'static str = stringify!($ty);

                fn from_relocation_value(value: i128) -> Result<Self, LinkError> {
                    <$ty>::try_from(value).map_err(|_| LinkError::RelocationValueOutOfRange {
                        type_name: Self::TYPE_NAME,
                        value,
                    })
                }

                fn to_relocation_value(&self) -> i128 {
                    *self as i128
                }
            }
        )*
    };
}

macro_rules! impl_optional_zero_sentinel {
    ($($ty:ty),* $(,)?) => {
        $(
            impl OptionalOperand for $ty {
                fn encode_optional<S: ByteSink + ?Sized>(
                    value: Option<&Self>,
                    width: OperandWidth,
                    output: &mut S,
                ) {
                    match value {
                        Some(v) => <Self as Operand>::encode(v, width, output),
                        None => <Self as Operand>::encode(&<$ty>::default(), width, output),
                    }
                }

                fn decode_optional(
                    width: OperandWidth,
                    input: &mut ByteCursor<'_>,
                ) -> Result<Option<Self>, DecodeError> {
                    let value = <Self as Operand>::decode(width, input)?;
                    if value == <$ty>::default() {
                        Ok(None)
                    } else {
                        Ok(Some(value))
                    }
                }

                fn fits_optional(value: Option<&Self>, width: OperandWidth) -> bool {
                    match value {
                        Some(v) => <Self as Operand>::fits_width(v, width),
                        None => true,
                    }
                }

                fn can_decode_from_ptr_optional() -> bool {
                    <Self as Operand>::can_decode_from_ptr()
                }

                unsafe fn decode_optional_from_ptr(
                    width: OperandWidth,
                    input: *const u8,
                ) -> (Option<Self>, *const u8) {
                    let (value, next) = unsafe { <Self as Operand>::decode_from_ptr(width, input) };
                    if value == <$ty>::default() {
                        (None, next)
                    } else {
                        (Some(value), next)
                    }
                }
            }
        )*
    };
}

impl_unsigned_encoding!(u8, u16, u32, usize);
impl_signed_encoding!(i8, i16, i32, isize);
impl_optional_zero_sentinel!(u8, u16, u32, usize, i8, i16, i32, isize);

impl Operand for bool {
    fn fits_width(_value: &Self, _width: OperandWidth) -> bool {
        true
    }

    fn encode<S: ByteSink + ?Sized>(value: &Self, width: OperandWidth, output: &mut S) {
        encode_unsigned(u32::from(*value), width, output)
    }

    fn decode(width: OperandWidth, input: &mut ByteCursor<'_>) -> Result<Self, DecodeError> {
        match input.read_unsigned(width)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(DecodeError::InvalidValue(
                "boolean operands must decode to 0 or 1",
            )),
        }
    }

    fn encoded_len(width: OperandWidth) -> usize {
        width.bytes()
    }

    fn can_decode_from_ptr() -> bool {
        true
    }

    unsafe fn decode_from_ptr(width: OperandWidth, input: *const u8) -> (Self, *const u8) {
        let (raw, next) = unsafe { <u32 as Operand>::decode_from_ptr(width, input) };
        #[cfg(debug_assertions)]
        assert_valid_bool_ptr_decode(raw);
        (raw != 0, next)
    }
}

impl RelocationField for bool {
    const TYPE_NAME: &'static str = "bool";

    fn from_relocation_value(value: i128) -> Result<Self, LinkError> {
        match value {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(LinkError::RelocationValueOutOfRange {
                type_name: Self::TYPE_NAME,
                value,
            }),
        }
    }

    fn to_relocation_value(&self) -> i128 {
        i128::from(u8::from(*self))
    }
}

impl OptionalOperand for bool {
    fn encode_optional<S: ByteSink + ?Sized>(
        value: Option<&Self>,
        width: OperandWidth,
        output: &mut S,
    ) {
        match value {
            Some(v) => <Self as Operand>::encode(v, width, output),
            None => encode_unsigned(0, width, output),
        }
    }

    fn decode_optional(
        width: OperandWidth,
        input: &mut ByteCursor<'_>,
    ) -> Result<Option<Self>, DecodeError> {
        match input.read_unsigned(width)? {
            0 => Ok(None),
            1 => Ok(Some(true)),
            _ => Err(DecodeError::InvalidValue(
                "optional boolean operands must decode to 0 or 1",
            )),
        }
    }

    fn fits_optional(value: Option<&Self>, width: OperandWidth) -> bool {
        match value {
            Some(v) => <Self as Operand>::fits_width(v, width),
            None => true,
        }
    }

    fn can_decode_from_ptr_optional() -> bool {
        true
    }

    unsafe fn decode_optional_from_ptr(
        width: OperandWidth,
        input: *const u8,
    ) -> (Option<Self>, *const u8) {
        let (raw, next) = unsafe { <u32 as Operand>::decode_from_ptr(width, input) };
        #[cfg(debug_assertions)]
        debug_assert!(raw <= 1, "optional boolean operands must decode to 0 or 1");
        match raw {
            0 => (None, next),
            _ => (Some(true), next),
        }
    }
}

impl<T> Operand for *const T {
    fn fits_width(value: &Self, width: OperandWidth) -> bool {
        <usize as Operand>::fits_width(&(*value as usize), width)
    }

    fn encode<S: ByteSink + ?Sized>(value: &Self, width: OperandWidth, output: &mut S) {
        <usize as Operand>::encode(&(*value as usize), width, output)
    }

    fn decode(width: OperandWidth, input: &mut ByteCursor<'_>) -> Result<Self, DecodeError> {
        Ok(<usize as Operand>::decode(width, input)? as *const T)
    }

    fn encoded_len(width: OperandWidth) -> usize {
        width.bytes()
    }

    fn can_decode_from_ptr() -> bool {
        true
    }

    unsafe fn decode_from_ptr(width: OperandWidth, input: *const u8) -> (Self, *const u8) {
        let (value, next) = unsafe { <usize as Operand>::decode_from_ptr(width, input) };
        (value as *const T, next)
    }
}

impl<T> Operand for *mut T {
    fn fits_width(value: &Self, width: OperandWidth) -> bool {
        <usize as Operand>::fits_width(&(*value as usize), width)
    }

    fn encode<S: ByteSink + ?Sized>(value: &Self, width: OperandWidth, output: &mut S) {
        <usize as Operand>::encode(&(*value as usize), width, output)
    }

    fn decode(width: OperandWidth, input: &mut ByteCursor<'_>) -> Result<Self, DecodeError> {
        Ok(<usize as Operand>::decode(width, input)? as *mut T)
    }

    fn encoded_len(width: OperandWidth) -> usize {
        width.bytes()
    }

    fn can_decode_from_ptr() -> bool {
        true
    }

    unsafe fn decode_from_ptr(width: OperandWidth, input: *const u8) -> (Self, *const u8) {
        let (value, next) = unsafe { <usize as Operand>::decode_from_ptr(width, input) };
        (value as *mut T, next)
    }
}

impl<T> Operand for NonNull<T> {
    fn fits_width(value: &Self, width: OperandWidth) -> bool {
        <usize as Operand>::fits_width(&(value.as_ptr() as usize), width)
    }

    fn encode<S: ByteSink + ?Sized>(value: &Self, width: OperandWidth, output: &mut S) {
        <usize as Operand>::encode(&(value.as_ptr() as usize), width, output)
    }

    fn decode(width: OperandWidth, input: &mut ByteCursor<'_>) -> Result<Self, DecodeError> {
        let pointer = <usize as Operand>::decode(width, input)? as *mut T;
        NonNull::new(pointer).ok_or(DecodeError::InvalidValue(
            "non-null pointer operand decoded to null",
        ))
    }

    fn encoded_len(width: OperandWidth) -> usize {
        width.bytes()
    }

    fn can_decode_from_ptr() -> bool {
        true
    }

    unsafe fn decode_from_ptr(width: OperandWidth, input: *const u8) -> (Self, *const u8) {
        let (pointer, next) = unsafe { <usize as Operand>::decode_from_ptr(width, input) };
        let pointer = pointer as *mut T;
        #[cfg(debug_assertions)]
        assert_non_null_ptr_decode(pointer);
        let pointer = NonNull::new(pointer).unwrap_or_else(NonNull::dangling);
        (pointer, next)
    }
}
