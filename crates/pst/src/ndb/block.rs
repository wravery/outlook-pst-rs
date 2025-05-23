//! [Blocks](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/a9c1981d-d1ea-457c-b39e-dc7fb0eb95d4)

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};

use super::{block_id::*, block_ref::*, byte_index::*, node_id::*, page::*, read_write::*, *};

pub const MAX_BLOCK_SIZE: u16 = 8192;

pub const fn block_size(size: u16) -> u16 {
    if size >= MAX_BLOCK_SIZE {
        MAX_BLOCK_SIZE
    } else {
        let size = if size < 64 { 64 } else { size };
        let tail = size % 64;
        if tail == 0 {
            size
        } else {
            size - tail + 64
        }
    }
}

/// [BLOCKTRAILER](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/a14943ef-70c2-403f-898c-5bc3747117e1)
pub trait BlockTrailer {
    type BlockId: BlockId;

    fn size(&self) -> u16;
    fn signature(&self) -> u16;
    fn crc(&self) -> u32;
    fn block_id(&self) -> Self::BlockId;
    fn cyclic_key(&self) -> u32;
    fn verify_block_id(&self, is_internal: bool) -> NdbResult<()>;
}

#[derive(Clone, Copy, Default)]
pub struct UnicodeBlockTrailer {
    size: u16,
    signature: u16,
    crc: u32,
    block_id: UnicodeBlockId,
}

impl UnicodeBlockTrailer {
    pub fn new(size: u16, signature: u16, crc: u32, block_id: UnicodeBlockId) -> NdbResult<Self> {
        if !(1..=(MAX_BLOCK_SIZE - Self::SIZE)).contains(&size) {
            return Err(NdbError::InvalidBlockSize(size));
        }

        Ok(Self {
            size,
            block_id,
            signature,
            crc,
        })
    }
}

impl BlockTrailer for UnicodeBlockTrailer {
    type BlockId = UnicodeBlockId;

    fn size(&self) -> u16 {
        self.size
    }

    fn signature(&self) -> u16 {
        self.signature
    }

    fn crc(&self) -> u32 {
        self.crc
    }

    fn block_id(&self) -> UnicodeBlockId {
        self.block_id
    }

    fn cyclic_key(&self) -> u32 {
        u64::from(self.block_id) as u32
    }

    fn verify_block_id(&self, is_internal: bool) -> NdbResult<()> {
        if self.block_id.is_internal() != is_internal {
            return Err(NdbError::InvalidUnicodeBlockTrailerId(u64::from(
                self.block_id,
            )));
        }
        Ok(())
    }
}

impl BlockTrailerReadWrite for UnicodeBlockTrailer {
    const SIZE: u16 = 16;

    fn new(size: u16, signature: u16, crc: u32, block_id: UnicodeBlockId) -> NdbResult<Self> {
        Self::new(size, signature, crc, block_id)
    }

    fn read(f: &mut dyn Read) -> io::Result<Self> {
        let size = f.read_u16::<LittleEndian>()?;
        if !(1..=(MAX_BLOCK_SIZE - Self::SIZE)).contains(&size) {
            return Err(NdbError::InvalidBlockSize(size).into());
        }

        let signature = f.read_u16::<LittleEndian>()?;
        let crc = f.read_u32::<LittleEndian>()?;
        let block_id = UnicodeBlockId::read(f)?;

        Ok(Self {
            size,
            signature,
            crc,
            block_id,
        })
    }

    fn write(&self, f: &mut dyn Write) -> io::Result<()> {
        f.write_u16::<LittleEndian>(self.size)?;
        f.write_u16::<LittleEndian>(self.signature)?;
        f.write_u32::<LittleEndian>(self.crc)?;
        self.block_id.write(f)
    }
}

#[derive(Clone, Copy, Default)]
pub struct AnsiBlockTrailer {
    size: u16,
    signature: u16,
    block_id: AnsiBlockId,
    crc: u32,
}

impl AnsiBlockTrailer {
    pub fn new(size: u16, signature: u16, crc: u32, block_id: AnsiBlockId) -> NdbResult<Self> {
        if !(1..=(MAX_BLOCK_SIZE - Self::SIZE)).contains(&size) {
            return Err(NdbError::InvalidBlockSize(size));
        }

        Ok(Self {
            size,
            signature,
            block_id,
            crc,
        })
    }
}

impl BlockTrailer for AnsiBlockTrailer {
    type BlockId = AnsiBlockId;

    fn size(&self) -> u16 {
        self.size
    }

    fn signature(&self) -> u16 {
        self.signature
    }

    fn crc(&self) -> u32 {
        self.crc
    }

    fn block_id(&self) -> AnsiBlockId {
        self.block_id
    }

    fn cyclic_key(&self) -> u32 {
        u32::from(self.block_id)
    }

    fn verify_block_id(&self, is_internal: bool) -> NdbResult<()> {
        if self.block_id.is_internal() != is_internal {
            return Err(NdbError::InvalidAnsiBlockTrailerId(u32::from(
                self.block_id,
            )));
        }
        Ok(())
    }
}

impl BlockTrailerReadWrite for AnsiBlockTrailer {
    const SIZE: u16 = 12;

    fn new(size: u16, signature: u16, crc: u32, block_id: AnsiBlockId) -> NdbResult<Self> {
        Self::new(size, signature, crc, block_id)
    }

    fn read(f: &mut dyn Read) -> io::Result<Self> {
        let size = f.read_u16::<LittleEndian>()?;
        if !(1..=(MAX_BLOCK_SIZE - Self::SIZE)).contains(&size) {
            return Err(NdbError::InvalidBlockSize(size).into());
        }

        let signature = f.read_u16::<LittleEndian>()?;
        let block_id = AnsiBlockId::read(f)?;
        let crc = f.read_u32::<LittleEndian>()?;

        Ok(Self {
            size,
            signature,
            block_id,
            crc,
        })
    }

    fn write(&self, f: &mut dyn Write) -> io::Result<()> {
        f.write_u16::<LittleEndian>(self.size)?;
        f.write_u16::<LittleEndian>(self.signature)?;
        self.block_id.write(f)?;
        f.write_u32::<LittleEndian>(self.crc)
    }
}

/// [Data Blocks](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/d0e6fbaf-00e3-4d4d-bea8-8ab3cdb4fde6)
pub trait Block {
    type Trailer: BlockTrailer;

    fn encoding(&self) -> NdbCryptMethod;
    fn data(&self) -> &[u8];
    fn trailer(&self) -> &Self::Trailer;
}

#[derive(Clone, Default)]
pub struct UnicodeDataBlock {
    encoding: NdbCryptMethod,
    data: Vec<u8>,
    trailer: UnicodeBlockTrailer,
}

impl UnicodeDataBlock {
    pub fn new(
        encoding: NdbCryptMethod,
        data: Vec<u8>,
        trailer: UnicodeBlockTrailer,
    ) -> NdbResult<Self> {
        trailer.verify_block_id(false)?;

        Ok(Self {
            data,
            encoding,
            trailer,
        })
    }
}

impl Block for UnicodeDataBlock {
    type Trailer = UnicodeBlockTrailer;

    fn encoding(&self) -> NdbCryptMethod {
        self.encoding
    }

    fn data(&self) -> &[u8] {
        &self.data
    }

    fn trailer(&self) -> &UnicodeBlockTrailer {
        &self.trailer
    }
}

impl BlockReadWrite for UnicodeDataBlock {
    fn new(
        encoding: NdbCryptMethod,
        data: Vec<u8>,
        trailer: UnicodeBlockTrailer,
    ) -> NdbResult<Self> {
        Self::new(encoding, data, trailer)
    }
}

impl From<UnicodeDataBlock> for Vec<u8> {
    fn from(value: UnicodeDataBlock) -> Self {
        value.data
    }
}

#[derive(Clone, Default)]
pub struct AnsiDataBlock {
    encoding: NdbCryptMethod,
    data: Vec<u8>,
    trailer: AnsiBlockTrailer,
}

impl AnsiDataBlock {
    pub fn new(
        encoding: NdbCryptMethod,
        data: Vec<u8>,
        trailer: AnsiBlockTrailer,
    ) -> NdbResult<Self> {
        trailer.verify_block_id(false)?;

        Ok(Self {
            data,
            encoding,
            trailer,
        })
    }
}

impl Block for AnsiDataBlock {
    type Trailer = AnsiBlockTrailer;

    fn encoding(&self) -> NdbCryptMethod {
        self.encoding
    }

    fn data(&self) -> &[u8] {
        &self.data
    }

    fn trailer(&self) -> &AnsiBlockTrailer {
        &self.trailer
    }
}

impl BlockReadWrite for AnsiDataBlock {
    fn new(encoding: NdbCryptMethod, data: Vec<u8>, trailer: AnsiBlockTrailer) -> NdbResult<Self> {
        Self::new(encoding, data, trailer)
    }
}

impl From<AnsiDataBlock> for Vec<u8> {
    fn from(value: AnsiDataBlock) -> Self {
        value.data
    }
}

pub trait IntermediateTreeHeader {
    fn level(&self) -> u8;
    fn entry_count(&self) -> u16;
}

pub trait IntermediateTreeEntry {}

pub trait IntermediateTreeBlock {
    type Header: IntermediateTreeHeader;
    type Entry: IntermediateTreeEntry;
    type Trailer: BlockTrailer;

    fn header(&self) -> &Self::Header;
    fn entries(&self) -> &[Self::Entry];
    fn trailer(&self) -> &Self::Trailer;
}

#[derive(Clone, Copy, Default)]
pub struct DataTreeBlockHeader {
    level: u8,
    entry_count: u16,
    total_size: u32,
}

impl DataTreeBlockHeader {
    pub fn new(level: u8, entry_count: u16, total_size: u32) -> Self {
        Self {
            level,
            entry_count,
            total_size,
        }
    }

    pub fn total_size(&self) -> u32 {
        self.total_size
    }
}

impl IntermediateTreeHeader for DataTreeBlockHeader {
    fn level(&self) -> u8 {
        self.level
    }

    fn entry_count(&self) -> u16 {
        self.entry_count
    }
}

impl IntermediateTreeHeaderReadWrite for DataTreeBlockHeader {
    const HEADER_SIZE: u16 = 8;

    fn read(f: &mut dyn Read) -> io::Result<Self> {
        let block_type = f.read_u8()?;
        if block_type != 0x01 {
            return Err(NdbError::InvalidInternalBlockType(block_type).into());
        }

        let level = f.read_u8()?;
        let entry_count = f.read_u16::<LittleEndian>()?;
        let total_size = f.read_u32::<LittleEndian>()?;

        Ok(Self {
            level,
            entry_count,
            total_size,
        })
    }

    fn write(&self, f: &mut dyn Write) -> io::Result<()> {
        f.write_u8(0x01)?;
        f.write_u8(self.level)?;
        f.write_u16::<LittleEndian>(self.entry_count)?;
        f.write_u32::<LittleEndian>(self.total_size)
    }
}

#[derive(Clone, Copy, Default)]
pub struct UnicodeDataTreeEntry(UnicodeBlockId);

impl UnicodeDataTreeEntry {
    pub fn new(block: UnicodeBlockId) -> Self {
        Self(block)
    }

    pub fn block(&self) -> UnicodeBlockId {
        self.0
    }
}

impl IntermediateTreeEntry for UnicodeDataTreeEntry {}

impl IntermediateTreeEntryReadWrite for UnicodeDataTreeEntry {
    const ENTRY_SIZE: u16 = 8;

    fn read(f: &mut dyn Read) -> io::Result<Self> {
        Ok(Self(UnicodeBlockId::read(f)?))
    }

    fn write(&self, f: &mut dyn Write) -> io::Result<()> {
        self.0.write(f)
    }
}

impl From<UnicodeBlockId> for UnicodeDataTreeEntry {
    fn from(value: UnicodeBlockId) -> Self {
        Self::new(value)
    }
}

impl From<UnicodeDataTreeEntry> for UnicodeBlockId {
    fn from(value: UnicodeDataTreeEntry) -> Self {
        value.block()
    }
}

#[derive(Clone, Copy, Default)]
pub struct AnsiDataTreeEntry(AnsiBlockId);

impl AnsiDataTreeEntry {
    pub fn new(block: AnsiBlockId) -> Self {
        Self(block)
    }

    pub fn block(&self) -> AnsiBlockId {
        self.0
    }
}

impl IntermediateTreeEntry for AnsiDataTreeEntry {}

impl IntermediateTreeEntryReadWrite for AnsiDataTreeEntry {
    const ENTRY_SIZE: u16 = 4;

    fn read(f: &mut dyn Read) -> io::Result<Self> {
        Ok(Self(AnsiBlockId::read(f)?))
    }

    fn write(&self, f: &mut dyn Write) -> io::Result<()> {
        self.0.write(f)
    }
}

impl From<AnsiBlockId> for AnsiDataTreeEntry {
    fn from(value: AnsiBlockId) -> Self {
        Self::new(value)
    }
}

impl From<AnsiDataTreeEntry> for AnsiBlockId {
    fn from(value: AnsiDataTreeEntry) -> Self {
        value.block()
    }
}

/// [XBLOCK](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/5b7a6935-e83d-4917-9f62-6ce3707f09e0)
/// / [XXBLOCK](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/061b6ac4-d1da-468c-b75d-0303a0a8f468)
struct DataTreeBlockInner<Entry, Trailer>
where
    Entry: IntermediateTreeEntry,
    Trailer: BlockTrailer,
{
    header: DataTreeBlockHeader,
    entries: Vec<Entry>,
    trailer: Trailer,
}

impl<Entry, Trailer> DataTreeBlockInner<Entry, Trailer>
where
    Entry: IntermediateTreeEntry,
    Trailer: BlockTrailer,
{
    pub fn new(
        header: DataTreeBlockHeader,
        entries: Vec<Entry>,
        trailer: Trailer,
    ) -> NdbResult<Self> {
        trailer.verify_block_id(true)?;

        Ok(Self {
            header,
            entries,
            trailer,
        })
    }
}

pub struct UnicodeDataTreeBlock {
    inner: DataTreeBlockInner<UnicodeDataTreeEntry, UnicodeBlockTrailer>,
}

impl IntermediateTreeBlock for UnicodeDataTreeBlock {
    type Header = DataTreeBlockHeader;
    type Entry = UnicodeDataTreeEntry;
    type Trailer = UnicodeBlockTrailer;

    fn header(&self) -> &Self::Header {
        &self.inner.header
    }

    fn entries(&self) -> &[Self::Entry] {
        &self.inner.entries
    }

    fn trailer(&self) -> &Self::Trailer {
        &self.inner.trailer
    }
}

impl IntermediateTreeBlockReadWrite for UnicodeDataTreeBlock {
    fn new(
        header: DataTreeBlockHeader,
        entries: Vec<UnicodeDataTreeEntry>,
        trailer: UnicodeBlockTrailer,
    ) -> NdbResult<Self> {
        Ok(Self {
            inner: DataTreeBlockInner::new(header, entries, trailer)?,
        })
    }
}

pub struct AnsiDataTreeBlock {
    inner: DataTreeBlockInner<AnsiDataTreeEntry, AnsiBlockTrailer>,
}

impl IntermediateTreeBlock for AnsiDataTreeBlock {
    type Header = DataTreeBlockHeader;
    type Entry = AnsiDataTreeEntry;
    type Trailer = AnsiBlockTrailer;

    fn header(&self) -> &Self::Header {
        &self.inner.header
    }

    fn entries(&self) -> &[Self::Entry] {
        &self.inner.entries
    }

    fn trailer(&self) -> &Self::Trailer {
        &self.inner.trailer
    }
}

impl IntermediateTreeBlockReadWrite for AnsiDataTreeBlock {
    fn new(
        header: DataTreeBlockHeader,
        entries: Vec<AnsiDataTreeEntry>,
        trailer: AnsiBlockTrailer,
    ) -> NdbResult<Self> {
        Ok(Self {
            inner: DataTreeBlockInner::new(header, entries, trailer)?,
        })
    }
}

pub enum UnicodeDataTree {
    Intermediate(Box<UnicodeDataTreeBlock>),
    Leaf(Box<UnicodeDataBlock>),
}

impl UnicodeDataTree {
    pub fn read<R: Read + Seek>(
        f: &mut R,
        encoding: NdbCryptMethod,
        block: &UnicodeBlockBTreeEntry,
    ) -> io::Result<Self> {
        f.seek(SeekFrom::Start(block.block().index().index()))?;

        let block_size = block_size(block.size() + UnicodeBlockTrailer::SIZE);
        let mut data = vec![0; block_size as usize];
        f.read_exact(&mut data)?;
        let mut cursor = Cursor::new(data);

        if block.block().block().is_internal() {
            let header = DataTreeBlockHeader::read(&mut cursor)?;
            cursor.seek(SeekFrom::Start(0))?;
            let block = UnicodeDataTreeBlock::read(&mut cursor, header, block.size())?;
            Ok(UnicodeDataTree::Intermediate(Box::new(block)))
        } else {
            let block = UnicodeDataBlock::read(&mut cursor, block.size(), encoding)?;
            Ok(UnicodeDataTree::Leaf(Box::new(block)))
        }
    }

    pub fn write<W: Write + Seek>(
        &self,
        f: &mut W,
        block: &UnicodeBlockBTreeEntry,
    ) -> io::Result<()> {
        f.seek(SeekFrom::Start(block.block().index().index()))?;

        match self {
            UnicodeDataTree::Intermediate(block) => block.write(f),
            UnicodeDataTree::Leaf(block) => block.write(f),
        }
    }

    pub fn blocks<R: Read + Seek>(
        &self,
        f: &mut R,
        encoding: NdbCryptMethod,
        block_btree: &UnicodeBlockBTree,
    ) -> io::Result<Box<dyn Iterator<Item = UnicodeDataBlock>>> {
        match self {
            UnicodeDataTree::Intermediate(block) => {
                let blocks = block
                    .entries()
                    .iter()
                    .map(|entry| {
                        let data_block = block_btree.find_entry(f, u64::from(entry.block()))?;
                        let data_tree = UnicodeDataTree::read(&mut *f, encoding, &data_block)?;
                        data_tree.blocks(f, encoding, block_btree)
                    })
                    .collect::<io::Result<Vec<_>>>()?;
                Ok(Box::new(blocks.into_iter().flatten()))
            }
            UnicodeDataTree::Leaf(block) => Ok(Box::new(Some(block.as_ref()).cloned().into_iter())),
        }
    }
}

pub enum AnsiDataTree {
    Intermediate(Box<AnsiDataTreeBlock>),
    Leaf(Box<AnsiDataBlock>),
}

impl AnsiDataTree {
    pub fn read<R: Read + Seek>(
        f: &mut R,
        encoding: NdbCryptMethod,
        block: &AnsiBlockBTreeEntry,
    ) -> io::Result<Self> {
        f.seek(SeekFrom::Start(u64::from(block.block().index().index())))?;

        let block_size = block_size(block.size() + AnsiBlockTrailer::SIZE);
        let mut data = vec![0; block_size as usize];
        f.read_exact(&mut data)?;
        let mut cursor = Cursor::new(data);

        if block.block().block().is_internal() {
            let header = DataTreeBlockHeader::read(&mut cursor)?;
            cursor.seek(SeekFrom::Start(0))?;
            let block = AnsiDataTreeBlock::read(&mut cursor, header, block.size())?;
            Ok(AnsiDataTree::Intermediate(Box::new(block)))
        } else {
            let block = AnsiDataBlock::read(&mut cursor, block.size(), encoding)?;
            Ok(AnsiDataTree::Leaf(Box::new(block)))
        }
    }

    pub fn write<W: Write + Seek>(&self, f: &mut W, block: &AnsiBlockBTreeEntry) -> io::Result<()> {
        f.seek(SeekFrom::Start(u64::from(block.block().index().index())))?;

        match self {
            AnsiDataTree::Intermediate(block) => block.write(f),
            AnsiDataTree::Leaf(block) => block.write(f),
        }
    }

    pub fn blocks<R: Read + Seek>(
        &self,
        f: &mut R,
        encoding: NdbCryptMethod,
        block_btree: &AnsiBlockBTree,
    ) -> io::Result<Box<dyn Iterator<Item = AnsiDataBlock>>> {
        match self {
            AnsiDataTree::Intermediate(block) => {
                let blocks = block
                    .entries()
                    .iter()
                    .map(|entry| {
                        let data_block = block_btree.find_entry(f, u32::from(entry.block()))?;
                        let data_tree = AnsiDataTree::read(&mut *f, encoding, &data_block)?;
                        data_tree.blocks(f, encoding, block_btree)
                    })
                    .collect::<io::Result<Vec<_>>>()?;
                Ok(Box::new(blocks.into_iter().flatten()))
            }
            AnsiDataTree::Leaf(block) => Ok(Box::new(Some(block.as_ref()).cloned().into_iter())),
        }
    }
}

#[derive(Clone, Copy, Default)]
struct SubNodeTreeBlockHeader {
    level: u8,
    entry_count: u16,
}

#[derive(Clone, Copy, Default)]
pub struct UnicodeSubNodeTreeBlockHeader(SubNodeTreeBlockHeader);

impl UnicodeSubNodeTreeBlockHeader {
    pub fn new(level: u8, entry_count: u16) -> Self {
        Self(SubNodeTreeBlockHeader { level, entry_count })
    }
}

impl IntermediateTreeHeader for UnicodeSubNodeTreeBlockHeader {
    fn level(&self) -> u8 {
        self.0.level
    }

    fn entry_count(&self) -> u16 {
        self.0.entry_count
    }
}

impl IntermediateTreeHeaderReadWrite for UnicodeSubNodeTreeBlockHeader {
    const HEADER_SIZE: u16 = 8;

    fn read(f: &mut dyn Read) -> io::Result<Self> {
        let block_type = f.read_u8()?;
        if block_type != 0x02 {
            return Err(NdbError::InvalidInternalBlockType(block_type).into());
        }

        let level = f.read_u8()?;
        let entry_count = f.read_u16::<LittleEndian>()?;

        let padding = f.read_u32::<LittleEndian>()?;
        if padding != 0 {
            return Err(NdbError::InvalidSubNodeBlockPadding(padding).into());
        }

        Ok(Self::new(level, entry_count))
    }

    fn write(&self, f: &mut dyn Write) -> io::Result<()> {
        f.write_u8(0x02)?;
        f.write_u8(self.level())?;
        f.write_u16::<LittleEndian>(self.entry_count())?;
        f.write_u32::<LittleEndian>(0)
    }
}

#[derive(Clone, Copy, Default)]
pub struct AnsiSubNodeTreeBlockHeader(SubNodeTreeBlockHeader);

impl AnsiSubNodeTreeBlockHeader {
    pub fn new(level: u8, entry_count: u16) -> Self {
        Self(SubNodeTreeBlockHeader { level, entry_count })
    }
}

impl IntermediateTreeHeader for AnsiSubNodeTreeBlockHeader {
    fn level(&self) -> u8 {
        self.0.level
    }

    fn entry_count(&self) -> u16 {
        self.0.entry_count
    }
}

impl IntermediateTreeHeaderReadWrite for AnsiSubNodeTreeBlockHeader {
    const HEADER_SIZE: u16 = 4;

    fn read(f: &mut dyn Read) -> io::Result<Self> {
        let block_type = f.read_u8()?;
        if block_type != 0x02 {
            return Err(NdbError::InvalidInternalBlockType(block_type).into());
        }

        let level = f.read_u8()?;
        let entry_count = f.read_u16::<LittleEndian>()?;

        Ok(Self::new(level, entry_count))
    }

    fn write(&self, f: &mut dyn Write) -> io::Result<()> {
        f.write_u8(0x02)?;
        f.write_u8(self.level())?;
        f.write_u16::<LittleEndian>(self.entry_count())
    }
}

/// [SLENTRY (Leaf Block Entry)](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/85c4d943-0779-43c5-bd98-61dc9bb5dfd6)
#[derive(Clone, Copy, Default)]
pub struct LeafSubNodeTreeEntry<Block>
where
    Block: BlockId,
{
    inner: IntermediateSubNodeTreeEntry<Block>,
    sub_node: Option<Block>,
}

impl<Block> LeafSubNodeTreeEntry<Block>
where
    Block: BlockId,
{
    pub fn new(node: NodeId, block: Block, sub_node: Option<Block>) -> Self {
        Self {
            inner: IntermediateSubNodeTreeEntry::new(node, block),
            sub_node,
        }
    }

    pub fn node(&self) -> NodeId {
        self.inner.node()
    }

    pub fn block(&self) -> Block {
        self.inner.block()
    }

    pub fn sub_node(&self) -> Option<Block> {
        self.sub_node
    }
}

pub type UnicodeLeafSubNodeTreeEntry = LeafSubNodeTreeEntry<UnicodeBlockId>;
pub type AnsiLeafSubNodeTreeEntry = LeafSubNodeTreeEntry<AnsiBlockId>;

impl IntermediateTreeEntry for UnicodeLeafSubNodeTreeEntry {}

impl IntermediateTreeEntryReadWrite for UnicodeLeafSubNodeTreeEntry {
    const ENTRY_SIZE: u16 = 24;

    fn read(f: &mut dyn Read) -> io::Result<Self> {
        let inner = UnicodeIntermediateSubNodeTreeEntry::read(f)?;
        let sub_node = UnicodeBlockId::read(f)?;
        let sub_node = if u64::from(sub_node) != 0 {
            Some(sub_node)
        } else {
            None
        };

        Ok(Self::new(inner.node(), inner.block(), sub_node))
    }

    fn write(&self, f: &mut dyn Write) -> io::Result<()> {
        self.inner.write(f)?;
        self.sub_node.unwrap_or_default().write(f)
    }
}

impl IntermediateTreeEntry for AnsiLeafSubNodeTreeEntry {}

impl IntermediateTreeEntryReadWrite for AnsiLeafSubNodeTreeEntry {
    const ENTRY_SIZE: u16 = 12;

    fn read(f: &mut dyn Read) -> io::Result<Self> {
        let inner = AnsiIntermediateSubNodeTreeEntry::read(f)?;
        let sub_node = AnsiBlockId::read(f)?;
        let sub_node = if u32::from(sub_node) != 0 {
            Some(sub_node)
        } else {
            None
        };

        Ok(Self::new(inner.node(), inner.block(), sub_node))
    }

    fn write(&self, f: &mut dyn Write) -> io::Result<()> {
        self.inner.write(f)?;
        self.sub_node.unwrap_or_default().write(f)
    }
}

/// [SIENTRY (Intermediate Block Entry)](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/9e79c673-d2f4-49fb-a00b-51b08fd2d1e4)
#[derive(Clone, Copy, Default)]
pub struct IntermediateSubNodeTreeEntry<Block>
where
    Block: BlockId,
{
    node: NodeId,
    block: Block,
}

impl<Block> IntermediateSubNodeTreeEntry<Block>
where
    Block: BlockId,
{
    pub fn new(node: NodeId, block: Block) -> Self {
        Self { node, block }
    }

    pub fn node(&self) -> NodeId {
        self.node
    }

    pub fn block(&self) -> Block {
        self.block
    }
}

pub type UnicodeIntermediateSubNodeTreeEntry = IntermediateSubNodeTreeEntry<UnicodeBlockId>;
pub type AnsiIntermediateSubNodeTreeEntry = IntermediateSubNodeTreeEntry<AnsiBlockId>;

impl IntermediateTreeEntry for UnicodeIntermediateSubNodeTreeEntry {}

impl IntermediateTreeEntryReadWrite for UnicodeIntermediateSubNodeTreeEntry {
    const ENTRY_SIZE: u16 = 16;

    fn read(f: &mut dyn Read) -> io::Result<Self> {
        let node = NodeId::from(f.read_u64::<LittleEndian>()? as u32);
        let block = UnicodeBlockId::read(f)?;
        Ok(Self::new(node, block))
    }

    fn write(&self, f: &mut dyn Write) -> io::Result<()> {
        f.write_u64::<LittleEndian>(u64::from(u32::from(self.node)))?;
        self.block.write(f)
    }
}

impl IntermediateTreeEntry for AnsiIntermediateSubNodeTreeEntry {}

impl IntermediateTreeEntryReadWrite for AnsiIntermediateSubNodeTreeEntry {
    const ENTRY_SIZE: u16 = 8;

    fn read(f: &mut dyn Read) -> io::Result<Self> {
        let node = NodeId::read(f)?;
        let block = AnsiBlockId::read(f)?;
        Ok(Self::new(node, block))
    }

    fn write(&self, f: &mut dyn Write) -> io::Result<()> {
        self.node.write(f)?;
        self.block.write(f)
    }
}

/// [SLBLOCK](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/5182eb24-4b0b-4816-aa3f-719cc6e6b018)
/// / [SIBLOCK](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/729fb9bd-060a-4bbc-9b3b-8f014b487dad)
pub struct SubNodeTreeBlock<Header, Entry, Trailer>
where
    Header: IntermediateTreeHeader,
    Entry: IntermediateTreeEntry,
    Trailer: BlockTrailer,
{
    header: Header,
    entries: Vec<Entry>,
    trailer: Trailer,
}

impl<Header, Entry, Trailer> SubNodeTreeBlock<Header, Entry, Trailer>
where
    Header: IntermediateTreeHeader,
    Entry: IntermediateTreeEntry,
    Trailer: BlockTrailer,
{
    pub fn new(header: Header, entries: Vec<Entry>, trailer: Trailer) -> NdbResult<Self> {
        trailer.verify_block_id(true)?;

        Ok(Self {
            header,
            entries,
            trailer,
        })
    }
}

impl<Header, Entry, Trailer> IntermediateTreeBlock for SubNodeTreeBlock<Header, Entry, Trailer>
where
    Header: IntermediateTreeHeader,
    Entry: IntermediateTreeEntry,
    Trailer: BlockTrailer,
{
    type Header = Header;
    type Entry = Entry;
    type Trailer = Trailer;

    fn header(&self) -> &Self::Header {
        &self.header
    }

    fn entries(&self) -> &[Self::Entry] {
        &self.entries
    }

    fn trailer(&self) -> &Trailer {
        &self.trailer
    }
}

impl<Header, Entry, Trailer> IntermediateTreeBlockReadWrite
    for SubNodeTreeBlock<Header, Entry, Trailer>
where
    Header: IntermediateTreeHeaderReadWrite,
    Entry: IntermediateTreeEntryReadWrite,
    Trailer: BlockTrailerReadWrite,
{
    fn new(header: Header, entries: Vec<Entry>, trailer: Trailer) -> NdbResult<Self> {
        Self::new(header, entries, trailer)
    }
}

pub type UnicodeIntermediateSubNodeTreeBlock = SubNodeTreeBlock<
    UnicodeSubNodeTreeBlockHeader,
    UnicodeIntermediateSubNodeTreeEntry,
    UnicodeBlockTrailer,
>;
pub type AnsiIntermediateSubNodeTreeBlock = SubNodeTreeBlock<
    AnsiSubNodeTreeBlockHeader,
    AnsiIntermediateSubNodeTreeEntry,
    AnsiBlockTrailer,
>;

pub type UnicodeLeafSubNodeTreeBlock = SubNodeTreeBlock<
    UnicodeSubNodeTreeBlockHeader,
    UnicodeLeafSubNodeTreeEntry,
    UnicodeBlockTrailer,
>;
pub type AnsiLeafSubNodeTreeBlock =
    SubNodeTreeBlock<AnsiSubNodeTreeBlockHeader, AnsiLeafSubNodeTreeEntry, AnsiBlockTrailer>;

pub enum UnicodeSubNodeTree {
    Intermediate(Box<UnicodeIntermediateSubNodeTreeBlock>),
    Leaf(Box<UnicodeLeafSubNodeTreeBlock>),
}

impl UnicodeSubNodeTree {
    pub fn read<R: Read + Seek>(f: &mut R, block: &UnicodeBlockBTreeEntry) -> io::Result<Self> {
        f.seek(SeekFrom::Start(block.block().index().index()))?;

        let block_size = block_size(block.size() + UnicodeBlockTrailer::SIZE);
        let mut data = vec![0; block_size as usize];
        f.read_exact(&mut data)?;
        let mut cursor = Cursor::new(data);
        let header = UnicodeSubNodeTreeBlockHeader::read(&mut cursor)?;
        cursor.seek(SeekFrom::Start(0))?;

        if header.level() > 0 {
            let block =
                UnicodeIntermediateSubNodeTreeBlock::read(&mut cursor, header, block.size())?;
            Ok(UnicodeSubNodeTree::Intermediate(Box::new(block)))
        } else {
            let block = UnicodeLeafSubNodeTreeBlock::read(&mut cursor, header, block.size())?;
            Ok(UnicodeSubNodeTree::Leaf(Box::new(block)))
        }
    }

    pub fn write<W: Write + Seek>(
        &self,
        f: &mut W,
        block: &UnicodeBlockBTreeEntry,
    ) -> io::Result<()> {
        f.seek(SeekFrom::Start(block.block().index().index()))?;

        match self {
            UnicodeSubNodeTree::Intermediate(block) => block.write(f),
            UnicodeSubNodeTree::Leaf(block) => block.write(f),
        }
    }

    pub fn find_entry<R: Read + Seek>(
        &self,
        f: &mut R,
        block_btree: &UnicodeBlockBTree,
        node: NodeId,
    ) -> io::Result<UnicodeBlockId> {
        match self {
            UnicodeSubNodeTree::Intermediate(block) => {
                let entry = block
                    .entries()
                    .iter()
                    .take_while(|entry| u32::from(entry.node()) <= u32::from(node))
                    .last()
                    .ok_or(NdbError::SubNodeNotFound(node))?;
                let block = block_btree.find_entry(f, u64::from(entry.block()))?;
                let page = Self::read(f, &block)?;
                page.find_entry(f, block_btree, node)
            }
            UnicodeSubNodeTree::Leaf(block) => {
                let entry = block
                    .entries()
                    .iter()
                    .find(|entry| u32::from(entry.node()) == u32::from(node))
                    .map(|entry| entry.block())
                    .ok_or(NdbError::SubNodeNotFound(node))?;
                Ok(entry)
            }
        }
    }

    pub fn entries<R: Read + Seek>(
        &self,
        f: &mut R,
        block_btree: &UnicodeBlockBTree,
    ) -> io::Result<Box<dyn Iterator<Item = UnicodeLeafSubNodeTreeEntry>>> {
        match self {
            UnicodeSubNodeTree::Intermediate(block) => {
                let entries = block
                    .entries()
                    .iter()
                    .map(|entry| {
                        let block = block_btree.find_entry(f, u64::from(entry.block()))?;
                        let sub_nodes = UnicodeSubNodeTree::read(f, &block)?;
                        sub_nodes.entries(f, block_btree)
                    })
                    .collect::<io::Result<Vec<_>>>()?;
                Ok(Box::new(entries.into_iter().flatten()))
            }
            UnicodeSubNodeTree::Leaf(block) => {
                let entries = block.entries().to_vec();
                Ok(Box::new(entries.into_iter()))
            }
        }
    }
}

pub enum AnsiSubNodeTree {
    Intermediate(Box<AnsiIntermediateSubNodeTreeBlock>),
    Leaf(Box<AnsiLeafSubNodeTreeBlock>),
}

impl AnsiSubNodeTree {
    pub fn read<R: Read + Seek>(f: &mut R, block: &AnsiBlockBTreeEntry) -> io::Result<Self> {
        f.seek(SeekFrom::Start(u64::from(block.block().index().index())))?;

        let block_size = block_size(block.size() + AnsiBlockTrailer::SIZE);
        let mut data = vec![0; block_size as usize];
        f.read_exact(&mut data)?;
        let mut cursor = Cursor::new(data);
        let header = AnsiSubNodeTreeBlockHeader::read(&mut cursor)?;
        cursor.seek(SeekFrom::Start(0))?;

        if header.level() > 0 {
            let block = AnsiIntermediateSubNodeTreeBlock::read(&mut cursor, header, block.size())?;
            Ok(AnsiSubNodeTree::Intermediate(Box::new(block)))
        } else {
            let block = AnsiLeafSubNodeTreeBlock::read(&mut cursor, header, block.size())?;
            Ok(AnsiSubNodeTree::Leaf(Box::new(block)))
        }
    }

    pub fn write<W: Write + Seek>(&self, f: &mut W, block: &AnsiBlockBTreeEntry) -> io::Result<()> {
        f.seek(SeekFrom::Start(u64::from(block.block().index().index())))?;

        match self {
            AnsiSubNodeTree::Intermediate(block) => block.write(f),
            AnsiSubNodeTree::Leaf(block) => block.write(f),
        }
    }

    pub fn find_entry<R: Read + Seek>(
        &self,
        f: &mut R,
        block_btree: &AnsiBlockBTree,
        node: NodeId,
    ) -> io::Result<AnsiBlockId> {
        match self {
            AnsiSubNodeTree::Intermediate(block) => {
                let entry = block
                    .entries()
                    .iter()
                    .take_while(|entry| u32::from(entry.node()) <= u32::from(node))
                    .last()
                    .ok_or(NdbError::SubNodeNotFound(node))?;
                let block = block_btree.find_entry(f, u32::from(entry.block()))?;
                let page = Self::read(f, &block)?;
                page.find_entry(f, block_btree, node)
            }
            AnsiSubNodeTree::Leaf(block) => {
                let entry = block
                    .entries()
                    .iter()
                    .find(|entry| u32::from(entry.node()) == u32::from(node))
                    .map(|entry| entry.block())
                    .ok_or(NdbError::SubNodeNotFound(node))?;
                Ok(entry)
            }
        }
    }

    pub fn entries<R: Read + Seek>(
        &self,
        f: &mut R,
        block_btree: &AnsiBlockBTree,
    ) -> io::Result<Box<dyn Iterator<Item = AnsiLeafSubNodeTreeEntry>>> {
        match self {
            AnsiSubNodeTree::Intermediate(block) => {
                let entries = block
                    .entries()
                    .iter()
                    .map(|entry| {
                        let block = block_btree.find_entry(f, u32::from(entry.block()))?;
                        let sub_nodes = AnsiSubNodeTree::read(f, &block)?;
                        sub_nodes.entries(f, block_btree)
                    })
                    .collect::<io::Result<Vec<_>>>()?;
                Ok(Box::new(entries.into_iter().flatten()))
            }
            AnsiSubNodeTree::Leaf(block) => {
                let entries = block.entries().to_vec();
                Ok(Box::new(entries.into_iter()))
            }
        }
    }
}
