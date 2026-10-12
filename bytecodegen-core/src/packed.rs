use std::collections::HashMap;
use std::fmt;

use crate::{DecodeError, EmitError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackedLayoutError {
    ZeroWordBits,
    WordBitsTooSmall {
        opcode_name: String,
    },
    ZeroWidthOperand {
        name: String,
    },
    ReservedOperandName,
    DuplicateOperand {
        opcode_name: String,
        name: String,
    },
    EmptyLayout,
    WordZeroMustStartWithOpcode,
    ZeroWidthField {
        word_index: usize,
    },
    UnknownOperandReference {
        opcode_name: String,
        name: String,
    },
    WordCoverageMismatch {
        word_index: usize,
        expected_bits: u8,
    },
    OperandCoverageMismatch {
        name: String,
        actual_bits: usize,
        expected_bits: usize,
    },
}

impl fmt::Display for PackedLayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroWordBits => write!(f, "packed layout requires a non-zero word size"),
            Self::WordBitsTooSmall { opcode_name } => {
                write!(f, "{opcode_name} requires word_bits >= 8 to encode op: 8")
            }
            Self::ZeroWidthOperand { name } => {
                write!(f, "operand `{name}` must have a non-zero packed width")
            }
            Self::ReservedOperandName => write!(
                f,
                "packed operands cannot be named `op` because that name is reserved for layout opcode fields"
            ),
            Self::DuplicateOperand { opcode_name, name } => {
                write!(f, "duplicate packed operand `{name}` in {opcode_name}")
            }
            Self::EmptyLayout => write!(f, "packed layout requires at least one word"),
            Self::WordZeroMustStartWithOpcode => write!(f, "word 0 must start with op: 8"),
            Self::ZeroWidthField { word_index } => {
                write!(f, "word {word_index} contains a zero-width field")
            }
            Self::UnknownOperandReference { opcode_name, name } => {
                write!(
                    f,
                    "{opcode_name} layout references unknown operand `{name}`"
                )
            }
            Self::WordCoverageMismatch {
                word_index,
                expected_bits,
            } => {
                write!(
                    f,
                    "word {word_index} must cover exactly {expected_bits} bits"
                )
            }
            Self::OperandCoverageMismatch {
                name,
                actual_bits,
                expected_bits,
            } => {
                write!(
                    f,
                    "operand `{name}` layout covers {actual_bits} bits but declares {expected_bits}"
                )
            }
        }
    }
}

impl std::error::Error for PackedLayoutError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackedOperandSpec {
    pub name: String,
    pub bit_width: u8,
}

impl PackedOperandSpec {
    pub fn required(name: impl Into<String>, bit_width: u8) -> Self {
        Self {
            name: name.into(),
            bit_width,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackedFieldRef {
    Opcode { bit_width: u8 },
    Padding { bit_width: u8 },
    Operand { name: String, bit_width: u8 },
}

impl PackedFieldRef {
    pub fn opcode(bit_width: u8) -> Self {
        Self::Opcode { bit_width }
    }

    pub fn padding(bit_width: u8) -> Self {
        Self::Padding { bit_width }
    }

    pub fn operand(name: impl Into<String>, bit_width: u8) -> Self {
        Self::Operand {
            name: name.into(),
            bit_width,
        }
    }

    fn bit_width(&self) -> u8 {
        match self {
            Self::Opcode { bit_width }
            | Self::Padding { bit_width }
            | Self::Operand { bit_width, .. } => *bit_width,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackedWordSpec {
    pub fields: Vec<PackedFieldRef>,
}

impl PackedWordSpec {
    pub fn new(fields: &[PackedFieldRef]) -> Self {
        Self {
            fields: fields.to_vec(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackedFieldPlacement {
    Opcode {
        word_index: usize,
        bit_offset: u8,
        bit_width: u8,
    },
    Operand {
        name: String,
        word_index: usize,
        bit_offset: u8,
        bit_width: u8,
        operand_bit_offset: u8,
    },
}

impl PackedFieldPlacement {
    pub fn opcode(word_index: usize, bit_offset: u8, bit_width: u8) -> Self {
        Self::Opcode {
            word_index,
            bit_offset,
            bit_width,
        }
    }

    pub fn operand(
        name: impl Into<String>,
        word_index: usize,
        bit_offset: u8,
        bit_width: u8,
        operand_bit_offset: u8,
    ) -> Self {
        Self::Operand {
            name: name.into(),
            word_index,
            bit_offset,
            bit_width,
            operand_bit_offset,
        }
    }

    pub fn as_ref(&self) -> PackedFieldPlacementRef<'_> {
        match self {
            Self::Opcode {
                word_index,
                bit_offset,
                bit_width,
            } => PackedFieldPlacementRef::Opcode {
                word_index: *word_index,
                bit_offset: *bit_offset,
                bit_width: *bit_width,
            },
            Self::Operand {
                name,
                word_index,
                bit_offset,
                bit_width,
                operand_bit_offset,
            } => PackedFieldPlacementRef::Operand {
                name,
                word_index: *word_index,
                bit_offset: *bit_offset,
                bit_width: *bit_width,
                operand_bit_offset: *operand_bit_offset,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackedFieldPlacementRef<'a> {
    Opcode {
        word_index: usize,
        bit_offset: u8,
        bit_width: u8,
    },
    Operand {
        name: &'a str,
        word_index: usize,
        bit_offset: u8,
        bit_width: u8,
        operand_bit_offset: u8,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackedLayoutPlan {
    pub word_bits: u8,
    pub word_count: usize,
    pub placements: Vec<PackedFieldPlacement>,
}

impl PackedLayoutPlan {
    pub fn as_ref(&self) -> PackedLayoutRef<'_> {
        PackedLayoutRef {
            word_bits: self.word_bits,
            word_count: self.word_count,
            placements: self
                .placements
                .iter()
                .map(PackedFieldPlacement::as_ref)
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackedLayoutRef<'a> {
    pub word_bits: u8,
    pub word_count: usize,
    pub placements: Vec<PackedFieldPlacementRef<'a>>,
}

pub fn plan_packed_layout(
    opcode_name: &str,
    word_bits: u8,
    operands: &[PackedOperandSpec],
    explicit_words: Option<&[PackedWordSpec]>,
) -> Result<PackedLayoutPlan, PackedLayoutError> {
    if word_bits == 0 {
        return Err(PackedLayoutError::ZeroWordBits);
    }
    if word_bits < 8 {
        return Err(PackedLayoutError::WordBitsTooSmall {
            opcode_name: opcode_name.to_owned(),
        });
    }

    let mut operand_bits = HashMap::new();
    for operand in operands {
        if operand.name == "op" {
            return Err(PackedLayoutError::ReservedOperandName);
        }
        if operand.bit_width == 0 {
            return Err(PackedLayoutError::ZeroWidthOperand {
                name: operand.name.clone(),
            });
        }
        if operand_bits
            .insert(operand.name.clone(), operand.bit_width)
            .is_some()
        {
            return Err(PackedLayoutError::DuplicateOperand {
                opcode_name: opcode_name.to_owned(),
                name: operand.name.clone(),
            });
        }
    }

    match explicit_words {
        Some(words) => {
            plan_explicit_packed_layout(opcode_name, word_bits, operands, &operand_bits, words)
        }
        None => Ok(plan_default_packed_layout(word_bits, operands)),
    }
}

fn plan_default_packed_layout(word_bits: u8, operands: &[PackedOperandSpec]) -> PackedLayoutPlan {
    let mut placements = vec![PackedFieldPlacement::opcode(0, 0, 8)];
    let mut cursor = 8usize;
    let word_bits = word_bits as usize;

    for operand in operands {
        let mut remaining = operand.bit_width as usize;
        let mut operand_bit_offset = 0usize;

        while remaining > 0 {
            let word_index = cursor / word_bits;
            let bit_offset = cursor % word_bits;
            let available = word_bits - bit_offset;
            let chunk = remaining.min(available);

            placements.push(PackedFieldPlacement::operand(
                operand.name.clone(),
                word_index,
                bit_offset as u8,
                chunk as u8,
                operand_bit_offset as u8,
            ));

            cursor += chunk;
            remaining -= chunk;
            operand_bit_offset += chunk;
        }
    }

    PackedLayoutPlan {
        word_bits: word_bits as u8,
        word_count: cursor.div_ceil(word_bits).max(1),
        placements,
    }
}

fn plan_explicit_packed_layout(
    opcode_name: &str,
    word_bits: u8,
    operands: &[PackedOperandSpec],
    operand_bits: &HashMap<String, u8>,
    words: &[PackedWordSpec],
) -> Result<PackedLayoutPlan, PackedLayoutError> {
    if words.is_empty() {
        return Err(PackedLayoutError::EmptyLayout);
    }

    match words.first().and_then(|word| word.fields.first()) {
        Some(PackedFieldRef::Opcode { bit_width: 8 }) => {}
        _ => {
            return Err(PackedLayoutError::WordZeroMustStartWithOpcode);
        }
    }

    let mut placements = Vec::new();
    let mut seen_bits = HashMap::<String, usize>::new();
    let mut opcode_seen = false;

    for (word_index, word) in words.iter().enumerate() {
        let mut bit_cursor = 0usize;

        for (field_index, field) in word.fields.iter().enumerate() {
            let field_width = field.bit_width();
            if field_width == 0 {
                return Err(PackedLayoutError::ZeroWidthField { word_index });
            }

            match field {
                PackedFieldRef::Opcode { bit_width } => {
                    if opcode_seen || word_index != 0 || field_index != 0 || *bit_width != 8 {
                        return Err(PackedLayoutError::WordZeroMustStartWithOpcode);
                    }
                    opcode_seen = true;
                    placements.push(PackedFieldPlacement::opcode(
                        word_index,
                        bit_cursor as u8,
                        *bit_width,
                    ));
                }
                PackedFieldRef::Padding { .. } => {}
                PackedFieldRef::Operand { name, bit_width } => {
                    let declared_bits = operand_bits.get(name).copied().ok_or_else(|| {
                        PackedLayoutError::UnknownOperandReference {
                            opcode_name: opcode_name.to_owned(),
                            name: name.clone(),
                        }
                    })? as usize;
                    let entry = seen_bits.entry(name.clone()).or_insert(0);
                    let next = *entry + *bit_width as usize;
                    if next > declared_bits {
                        return Err(PackedLayoutError::OperandCoverageMismatch {
                            name: name.clone(),
                            actual_bits: next,
                            expected_bits: declared_bits,
                        });
                    }
                    placements.push(PackedFieldPlacement::operand(
                        name.clone(),
                        word_index,
                        bit_cursor as u8,
                        *bit_width,
                        *entry as u8,
                    ));
                    *entry = next;
                }
            }

            bit_cursor += field_width as usize;
        }

        if bit_cursor != word_bits as usize {
            return Err(PackedLayoutError::WordCoverageMismatch {
                word_index,
                expected_bits: word_bits,
            });
        }
    }

    for operand in operands {
        let actual = seen_bits.get(&operand.name).copied().unwrap_or_default();
        let expected = operand.bit_width as usize;
        if actual == 0 {
            return Err(PackedLayoutError::OperandCoverageMismatch {
                name: operand.name.clone(),
                actual_bits: actual,
                expected_bits: expected,
            });
        }
    }

    Ok(PackedLayoutPlan {
        word_bits,
        word_count: words.len(),
        placements,
    })
}

pub trait PackedOperand: Sized + Copy {
    const PACKED_BITS: u8;

    fn encode_packed(value: &Self) -> Result<u32, EmitError>;
    fn decode_packed(raw: u32) -> Result<Self, DecodeError>;

    fn encode_packed_width(value: &Self, width: u8) -> Result<u32, EmitError> {
        if width == Self::PACKED_BITS {
            Self::encode_packed(value)
        } else {
            panic!(
                "encode_packed_width not implemented for {} at width {}",
                std::any::type_name::<Self>(),
                width
            )
        }
    }

    fn decode_packed_width(raw: u32, width: u8) -> Result<Self, DecodeError> {
        if width == Self::PACKED_BITS {
            Self::decode_packed(raw)
        } else {
            panic!(
                "decode_packed_width not implemented for {} at width {}",
                std::any::type_name::<Self>(),
                width
            )
        }
    }
}

pub trait PackedOptionalOperand: PackedOperand {
    fn encode_packed_optional(value: Option<&Self>) -> Result<u32, EmitError>;
    fn decode_packed_optional(raw: u32) -> Result<Option<Self>, DecodeError>;

    fn encode_packed_optional_width(value: Option<&Self>, width: u8) -> Result<u32, EmitError> {
        if width == Self::PACKED_BITS {
            Self::encode_packed_optional(value)
        } else {
            panic!(
                "encode_packed_optional_width not implemented for {} at width {}",
                std::any::type_name::<Self>(),
                width
            )
        }
    }

    fn decode_packed_optional_width(raw: u32, width: u8) -> Result<Option<Self>, DecodeError> {
        if width == Self::PACKED_BITS {
            Self::decode_packed_optional(raw)
        } else {
            panic!(
                "decode_packed_optional_width not implemented for {} at width {}",
                std::any::type_name::<Self>(),
                width
            )
        }
    }
}

pub trait PackedEncode {
    fn encode_packed_to_words(&self) -> Result<Vec<u32>, EmitError>;

    fn encode_packed_to_bytes(&self) -> Result<Vec<u8>, EmitError> {
        let words = self.encode_packed_to_words()?;
        Ok(packed_words_to_bytes(&words))
    }
}

pub trait PackedDecode: Sized {
    fn decode_packed_words(words: &[u32]) -> Result<Self, DecodeError>;

    fn decode_packed_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        let words = packed_bytes_to_words(bytes)?;
        Self::decode_packed_words(&words)
    }
}

pub trait PackedEmit: PackedEncode {
    fn packed_word_len(&self) -> usize;
}

pub const fn packed_mask(bit_width: u8) -> u32 {
    if bit_width == 0 {
        0
    } else if bit_width >= 32 {
        u32::MAX
    } else {
        (1u32 << bit_width) - 1
    }
}

const fn packed_bit_range_is_valid(bit_offset: u8, bit_width: u8) -> bool {
    let end = bit_offset as u16 + bit_width as u16;
    bit_width != 0 && bit_offset < 32 && end <= 32
}

/// Extracts a packed bit range from `word`.
///
/// Invalid offset/width combinations return `0` instead of panicking.
pub const fn extract_packed_bits(word: u32, bit_offset: u8, bit_width: u8) -> u32 {
    if !packed_bit_range_is_valid(bit_offset, bit_width) {
        0
    } else if bit_width >= 32 {
        word >> bit_offset
    } else {
        (word >> bit_offset) & packed_mask(bit_width)
    }
}

/// Inserts a packed bit range into `word`.
///
/// Invalid offset/width combinations return `word` unchanged instead of panicking.
pub const fn insert_packed_bits(word: u32, value: u32, bit_offset: u8, bit_width: u8) -> u32 {
    if !packed_bit_range_is_valid(bit_offset, bit_width) {
        word
    } else if bit_width >= 32 {
        value << bit_offset
    } else {
        let mask = packed_mask(bit_width) << bit_offset;
        (word & !mask) | ((value & packed_mask(bit_width)) << bit_offset)
    }
}

pub const fn sign_extend_packed(raw: u32, bit_width: u8) -> i32 {
    if bit_width == 0 || bit_width >= 32 {
        raw as i32
    } else {
        let shift = 32 - bit_width;
        ((raw << shift) as i32) >> shift
    }
}

pub fn packed_words_to_bytes(words: &[u32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(words.len() * 4);
    for word in words {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes
}

pub fn packed_bytes_to_words(bytes: &[u8]) -> Result<Vec<u32>, DecodeError> {
    if bytes.len() % 4 != 0 {
        return Err(DecodeError::InvalidValue(
            "packed byte input length must be a multiple of 4",
        ));
    }

    let mut words = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        words.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    Ok(words)
}

macro_rules! impl_packed_unsigned {
    ($ty:ty, $bits:expr) => {
        impl PackedOperand for $ty {
            const PACKED_BITS: u8 = $bits;

            fn encode_packed(value: &Self) -> Result<u32, EmitError> {
                Self::encode_packed_width(value, Self::PACKED_BITS)
            }

            fn encode_packed_width(value: &Self, width: u8) -> Result<u32, EmitError> {
                if (*value as u128) > (packed_mask(width) as u128) {
                    return Err(EmitError::PackedValueDoesNotFit {
                        type_name: stringify!($ty),
                        bit_width: width,
                    });
                }
                Ok(*value as u32)
            }

            fn decode_packed(raw: u32) -> Result<Self, DecodeError> {
                Self::decode_packed_width(raw, Self::PACKED_BITS)
            }

            fn decode_packed_width(raw: u32, width: u8) -> Result<Self, DecodeError> {
                if raw > packed_mask(width) {
                    return Err(DecodeError::InvalidValue(concat!(
                        "packed value does not fit ",
                        stringify!($ty)
                    )));
                }
                <$ty>::try_from(raw).map_err(|_| {
                    DecodeError::InvalidValue(concat!(
                        "packed value does not fit ",
                        stringify!($ty)
                    ))
                })
            }
        }
    };
}

macro_rules! impl_packed_signed {
    ($ty:ty, $bits:expr) => {
        impl PackedOperand for $ty {
            const PACKED_BITS: u8 = $bits;

            fn encode_packed(value: &Self) -> Result<u32, EmitError> {
                Self::encode_packed_width(value, Self::PACKED_BITS)
            }

            fn encode_packed_width(value: &Self, width: u8) -> Result<u32, EmitError> {
                let raw = *value as i128;
                let min = -(1i128 << (width - 1));
                let max = (1i128 << (width - 1)) - 1;
                if raw < min || raw > max {
                    return Err(EmitError::PackedValueDoesNotFit {
                        type_name: stringify!($ty),
                        bit_width: width,
                    });
                }
                Ok((raw as u32) & packed_mask(width))
            }

            fn decode_packed(raw: u32) -> Result<Self, DecodeError> {
                Self::decode_packed_width(raw, Self::PACKED_BITS)
            }

            fn decode_packed_width(raw: u32, width: u8) -> Result<Self, DecodeError> {
                if raw > packed_mask(width) {
                    return Err(DecodeError::InvalidValue(concat!(
                        "packed value does not fit ",
                        stringify!($ty)
                    )));
                }
                let value = sign_extend_packed(raw, width);
                <$ty>::try_from(value).map_err(|_| {
                    DecodeError::InvalidValue(concat!(
                        "packed value does not fit ",
                        stringify!($ty)
                    ))
                })
            }
        }
    };
}

macro_rules! impl_packed_optional_zero_sentinel {
    ($($ty:ty),* $(,)?) => {
        $(
            impl PackedOptionalOperand for $ty {
                fn encode_packed_optional(value: Option<&Self>) -> Result<u32, EmitError> {
                    Self::encode_packed_optional_width(value, Self::PACKED_BITS)
                }

                fn encode_packed_optional_width(value: Option<&Self>, width: u8) -> Result<u32, EmitError> {
                    match value {
                        Some(v) => <Self as PackedOperand>::encode_packed_width(v, width),
                        None => Ok(0),
                    }
                }

                fn decode_packed_optional(raw: u32) -> Result<Option<Self>, DecodeError> {
                    Self::decode_packed_optional_width(raw, Self::PACKED_BITS)
                }

                fn decode_packed_optional_width(raw: u32, width: u8) -> Result<Option<Self>, DecodeError> {
                    if raw == 0 {
                        Ok(None)
                    } else {
                        Ok(Some(<Self as PackedOperand>::decode_packed_width(raw, width)?))
                    }
                }
            }
        )*
    };
}

impl_packed_unsigned!(u8, 8);
impl_packed_unsigned!(u16, 16);
impl_packed_unsigned!(u32, 32);
impl_packed_unsigned!(usize, 32);

impl_packed_signed!(i8, 8);
impl_packed_signed!(i16, 16);
impl_packed_signed!(i32, 32);
impl_packed_signed!(isize, 32);

impl PackedOperand for bool {
    const PACKED_BITS: u8 = 8;

    fn encode_packed(value: &Self) -> Result<u32, EmitError> {
        Self::encode_packed_width(value, Self::PACKED_BITS)
    }

    fn encode_packed_width(value: &Self, _width: u8) -> Result<u32, EmitError> {
        Ok(u32::from(*value))
    }

    fn decode_packed(raw: u32) -> Result<Self, DecodeError> {
        Self::decode_packed_width(raw, Self::PACKED_BITS)
    }

    fn decode_packed_width(raw: u32, _width: u8) -> Result<Self, DecodeError> {
        match raw {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(DecodeError::InvalidValue(
                "packed boolean operands must decode to 0 or 1",
            )),
        }
    }
}

impl_packed_optional_zero_sentinel!(bool, u8, u16, u32, usize, i8, i16, i32, isize);
