//! [BID (Block ID)](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/d3155aa1-ccdd-4dee-a0a9-5363ccca5352)

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::{
    fmt::Debug,
    io::{self, Read, Write},
};

use super::{read_write::*, *};

pub trait BlockId: Copy + Sized {
    type Index: Copy + Sized + From<Self> + Into<u64>;

    fn is_internal(&self) -> bool;
    fn index(&self) -> Self::Index;
}

pub const MAX_UNICODE_BLOCK_INDEX: u64 = 1_u64.rotate_right(2) - 1;

#[derive(Clone, Copy, Default)]
pub struct UnicodeBlockId(u64);

impl UnicodeBlockId {
    pub fn new(is_internal: bool, index: u64) -> NdbResult<Self> {
        let is_internal = if is_internal { 0x2 } else { 0x0 };

        let shifted_index = index.rotate_left(2);
        if shifted_index & 0x3 != 0 {
            return Err(NdbError::InvalidUnicodeBlockIndex(index));
        };

        Ok(Self(shifted_index | is_internal))
    }
}

impl BlockId for UnicodeBlockId {
    type Index = u64;

    fn is_internal(&self) -> bool {
        self.0 & 0x2 == 0x2
    }

    fn index(&self) -> u64 {
        self.0 >> 2
    }
}

impl BlockIdReadWrite for UnicodeBlockId {
    fn read(f: &mut dyn Read) -> io::Result<Self> {
        let value = f.read_u64::<LittleEndian>()?;
        Ok(Self(value))
    }

    fn write(&self, f: &mut dyn Write) -> io::Result<()> {
        f.write_u64::<LittleEndian>(self.0)
    }
}

impl Debug for UnicodeBlockId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "UnicodeBlockId {{ {}: 0x{:X} }}",
            if self.is_internal() {
                "internal"
            } else {
                "leaf"
            },
            self.index()
        )
    }
}

impl From<u64> for UnicodeBlockId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<UnicodeBlockId> for u64 {
    fn from(value: UnicodeBlockId) -> Self {
        value.0 & !0x1
    }
}

pub const MAX_ANSI_BLOCK_INDEX: u32 = 1_u32.rotate_right(2) - 1;

#[derive(Clone, Copy, Default)]
pub struct AnsiBlockId(u32);

impl AnsiBlockId {
    pub fn new(is_internal: bool, index: u32) -> NdbResult<Self> {
        let is_internal = if is_internal { 0x2 } else { 0x0 };

        let shifted_index = index.rotate_left(2);
        if shifted_index & 0x3 != 0 {
            return Err(NdbError::InvalidAnsiBlockIndex(index));
        };

        Ok(Self(shifted_index | is_internal))
    }
}

impl BlockId for AnsiBlockId {
    type Index = u32;

    fn is_internal(&self) -> bool {
        self.0 & 0x2 == 0x2
    }

    fn index(&self) -> u32 {
        self.0 >> 2
    }
}

impl BlockIdReadWrite for AnsiBlockId {
    fn read(f: &mut dyn Read) -> io::Result<Self> {
        let value = f.read_u32::<LittleEndian>()?;
        Ok(Self(value))
    }

    fn write(&self, f: &mut dyn Write) -> io::Result<()> {
        f.write_u32::<LittleEndian>(self.0)
    }
}

impl Debug for AnsiBlockId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "AnsiBlockId {{ {}: 0x{:X} }}",
            if self.is_internal() {
                "internal"
            } else {
                "leaf"
            },
            self.index()
        )
    }
}

impl From<u32> for AnsiBlockId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<AnsiBlockId> for u32 {
    fn from(value: AnsiBlockId) -> Self {
        value.0 & !0x1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unicode_bid_index_overflow() {
        let Err(NdbError::InvalidUnicodeBlockIndex(value)) =
            UnicodeBlockId::new(false, MAX_UNICODE_BLOCK_INDEX + 1)
        else {
            panic!("UnicodeBlockId should be out of range");
        };
        assert_eq!(value, MAX_UNICODE_BLOCK_INDEX + 1);
    }

    #[test]
    fn test_ansi_bid_index_overflow() {
        let Err(NdbError::InvalidAnsiBlockIndex(value)) =
            AnsiBlockId::new(false, MAX_ANSI_BLOCK_INDEX + 1)
        else {
            panic!("AnsiBlockId should be out of range");
        };
        assert_eq!(value, MAX_ANSI_BLOCK_INDEX + 1);
    }
}
