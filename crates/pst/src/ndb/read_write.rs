use std::{
    cmp::Ordering,
    io::{self, Cursor, Read, Seek, SeekFrom, Write},
};

use super::{block::*, block_id::*, block_ref::*, byte_index::*, header::*, page::*, root::*, *};
use crate::{
    crc::compute_crc,
    encode::{cyclic, permute},
};

pub trait BlockIdReadWrite: BlockId + Copy + Sized {
    fn new(is_internal: bool, index: Self::Index) -> NdbResult<Self>;
    fn read(f: &mut dyn Read) -> io::Result<Self>;
    fn write(&self, f: &mut dyn Write) -> io::Result<()>;
}

pub trait ByteIndexReadWrite: ByteIndex + Copy + Sized {
    fn new(index: Self::Index) -> Self;
    fn read(f: &mut dyn Read) -> io::Result<Self>;
    fn write(&self, f: &mut dyn Write) -> io::Result<()>;
}

pub trait BlockRefReadWrite: BlockRef + Copy + Sized {
    fn new(block: Self::Block, index: Self::Index) -> Self;

    fn read(f: &mut dyn Read) -> io::Result<Self> {
        let block = Self::Block::read(f)?;
        let index = Self::Index::read(f)?;
        Ok(Self::new(block, index))
    }

    fn write(&self, f: &mut dyn Write) -> io::Result<()> {
        self.block().write(f)?;
        self.index().write(f)
    }
}

pub trait RootReadWrite: Root + Sized {
    fn new(
        file_eof_index: Self::Index,
        amap_last_index: Self::Index,
        amap_free_size: Self::Index,
        pmap_free_size: Self::Index,
        node_btree: Self::BTreeRef,
        block_btree: Self::BTreeRef,
        amap_is_valid: AmapStatus,
    ) -> Self;

    fn load_reserved(&mut self, reserved1: u32, reserved2: u8, reserved3: u16);

    fn reserved1(&self) -> u32;
    fn reserved2(&self) -> u8;
    fn reserved3(&self) -> u16;

    fn read(f: &mut dyn Read) -> io::Result<Self> {
        let reserved1 = f.read_u32::<LittleEndian>()?;
        let file_eof_index = Self::Index::read(f)?;
        let amap_last_index = Self::Index::read(f)?;
        let amap_free_size = Self::Index::read(f)?;
        let pmap_free_size = Self::Index::read(f)?;
        let node_btree = Self::BTreeRef::read(f)?;
        let block_btree = Self::BTreeRef::read(f)?;
        let amap_is_valid = AmapStatus::try_from(f.read_u8()?).unwrap_or(AmapStatus::Invalid);
        let reserved2 = f.read_u8()?;
        let reserved3 = f.read_u16::<LittleEndian>()?;
        let mut root = Self::new(
            file_eof_index,
            amap_last_index,
            amap_free_size,
            pmap_free_size,
            node_btree,
            block_btree,
            amap_is_valid,
        );
        root.load_reserved(reserved1, reserved2, reserved3);
        Ok(root)
    }

    fn write(&self, f: &mut dyn Write) -> io::Result<()> {
        f.write_u32::<LittleEndian>(self.reserved1())?;
        self.file_eof_index().write(f)?;
        self.amap_last_index().write(f)?;
        self.amap_free_size().write(f)?;
        self.pmap_free_size().write(f)?;
        self.node_btree().write(f)?;
        self.block_btree().write(f)?;
        f.write_u8(self.amap_is_valid() as u8)?;
        f.write_u8(self.reserved2())?;
        f.write_u16::<LittleEndian>(self.reserved3())
    }
}

pub trait HeaderReadWrite: Header + Sized {
    fn new(root: Self::Root, crypt_method: NdbCryptMethod) -> Self;
}

pub trait PageTrailerReadWrite: PageTrailer + Copy + Sized {
    fn new(page_type: PageType, signature: u16, block_id: Self::BlockId, crc: u32) -> Self;
    fn read(f: &mut dyn Read) -> io::Result<Self>;
    fn write(&self, f: &mut dyn Write) -> io::Result<()>;
}

pub trait MapPageReadWrite: MapPage + Sized {
    const PAGE_TYPE: u8;

    fn new(amap_bits: MapBits, trailer: Self::Trailer) -> NdbResult<Self>;
    fn read(f: &mut dyn Read) -> io::Result<Self>;
    fn write(&self, f: &mut dyn Write) -> io::Result<()>;
}

pub trait DensityListPageReadWrite: DensityListPage + Sized {
    fn new(
        backfill_complete: bool,
        current_page: u32,
        entries: &[DensityListPageEntry],
        trailer: Self::Trailer,
    ) -> NdbResult<Self>;
    fn read<R: Read + Seek>(f: &mut R) -> io::Result<Self>;
    fn write<W: Write + Seek>(&self, f: &mut W) -> io::Result<()>;
}

pub trait BTreePageReadWrite: BTreePage + Sized {
    fn new(level: u8, entries: &[Self::Entry], trailer: Self::Trailer) -> NdbResult<Self>;
}

pub const UNICODE_BTREE_ENTRIES_SIZE: usize = 488;

pub trait UnicodeBTreePageReadWrite<Entry>:
    BTreePageReadWrite<Entry = Entry, Trailer = UnicodePageTrailer> + Sized
where
    Entry: BTreeEntry,
{
    const MAX_BTREE_ENTRIES: usize = UNICODE_BTREE_ENTRIES_SIZE / Entry::ENTRY_SIZE;

    fn read(f: &mut dyn Read) -> io::Result<Self> {
        let mut buffer = [0_u8; 496];
        f.read_exact(&mut buffer)?;
        let mut cursor = Cursor::new(buffer);

        cursor.seek(SeekFrom::Start(UNICODE_BTREE_ENTRIES_SIZE as u64))?;

        // cEnt
        let entry_count = usize::from(cursor.read_u8()?);
        if entry_count > Self::MAX_BTREE_ENTRIES {
            return Err(NdbError::InvalidBTreeEntryCount(entry_count).into());
        }

        // cEntMax
        let max_entries = cursor.read_u8()?;
        if usize::from(max_entries) != Self::MAX_BTREE_ENTRIES {
            return Err(NdbError::InvalidBTreeEntryMaxCount(max_entries).into());
        }

        // cbEnt
        let entry_size = cursor.read_u8()?;
        if usize::from(entry_size) != Entry::ENTRY_SIZE {
            return Err(NdbError::InvalidBTreeEntrySize(entry_size).into());
        }

        // cLevel
        let level = cursor.read_u8()?;
        if !(0..=8).contains(&level) {
            return Err(NdbError::InvalidBTreePageLevel(level).into());
        }

        // dwPadding
        let padding = cursor.read_u32::<LittleEndian>()?;
        if padding != 0 {
            return Err(NdbError::InvalidBTreePagePadding(padding).into());
        }

        // pageTrailer
        let trailer = UnicodePageTrailer::read(f)?;
        if trailer.page_type() != PageType::BlockBTree && trailer.page_type() != PageType::NodeBTree
        {
            return Err(NdbError::UnexpectedPageType(trailer.page_type()).into());
        }

        let buffer = cursor.into_inner();
        let crc = compute_crc(0, &buffer);
        if crc != trailer.crc() {
            return Err(NdbError::InvalidPageCrc(crc).into());
        }

        // rgentries
        let mut cursor = Cursor::new(buffer);
        let mut entries = Vec::with_capacity(entry_count);
        for _ in 0..entry_count {
            entries.push(<Self::Entry as BTreeEntry>::read(&mut cursor)?);
        }

        Ok(<Self as BTreePageReadWrite>::new(level, &entries, trailer)?)
    }

    fn write(&self, f: &mut dyn Write) -> io::Result<()> {
        let mut cursor = Cursor::new([0_u8; 496]);

        // rgentries
        let entries = self.entries();
        for entry in entries.iter().take(Self::MAX_BTREE_ENTRIES) {
            <Self::Entry as BTreeEntry>::write(entry, &mut cursor)?;
        }
        if entries.len() < Self::MAX_BTREE_ENTRIES {
            let entry = Default::default();
            for _ in entries.len()..Self::MAX_BTREE_ENTRIES {
                <Self::Entry as BTreeEntry>::write(&entry, &mut cursor)?;
            }
        }

        // cEnt
        cursor.write_u8(entries.len() as u8)?;

        // cEntMax
        cursor.write_u8(Self::MAX_BTREE_ENTRIES as u8)?;

        // cbEnt
        cursor.write_u8(Entry::ENTRY_SIZE as u8)?;

        // cLevel
        cursor.write_u8(self.level())?;

        // dwPadding
        cursor.write_u32::<LittleEndian>(0)?;

        let buffer = cursor.into_inner();
        let crc = compute_crc(0, &buffer);

        f.write_all(&buffer)?;

        // pageTrailer
        let trailer = self.trailer();
        let trailer = UnicodePageTrailer::new(
            trailer.page_type(),
            trailer.signature(),
            trailer.block_id(),
            crc,
        );

        trailer.write(f)
    }
}

pub const ANSI_BTREE_ENTRIES_SIZE: usize = 496;

pub trait AnsiBTreePageReadWrite<Entry>:
    BTreePageReadWrite<Entry = Entry, Trailer = AnsiPageTrailer> + Sized
where
    Entry: BTreeEntry,
{
    const MAX_BTREE_ENTRIES: usize = ANSI_BTREE_ENTRIES_SIZE / Entry::ENTRY_SIZE;

    fn read(f: &mut dyn Read) -> io::Result<Self> {
        let mut buffer = [0_u8; 500];
        f.read_exact(&mut buffer)?;
        let mut cursor = Cursor::new(buffer);

        cursor.seek(SeekFrom::Start(ANSI_BTREE_ENTRIES_SIZE as u64))?;

        // cEnt
        let entry_count = usize::from(cursor.read_u8()?);
        if entry_count > Self::MAX_BTREE_ENTRIES {
            return Err(NdbError::InvalidBTreeEntryCount(entry_count).into());
        }

        // cEntMax
        let max_entries = cursor.read_u8()?;
        if usize::from(max_entries) != Self::MAX_BTREE_ENTRIES {
            return Err(NdbError::InvalidBTreeEntryMaxCount(max_entries).into());
        }

        // cbEnt
        let entry_size = cursor.read_u8()?;
        if usize::from(entry_size) != Entry::ENTRY_SIZE {
            return Err(NdbError::InvalidBTreeEntrySize(entry_size).into());
        }

        // cLevel
        let level = cursor.read_u8()?;
        if !(0..=8).contains(&level) {
            return Err(NdbError::InvalidBTreePageLevel(level).into());
        }

        // pageTrailer
        let trailer = AnsiPageTrailer::read(f)?;
        if trailer.page_type() != PageType::BlockBTree && trailer.page_type() != PageType::NodeBTree
        {
            return Err(NdbError::UnexpectedPageType(trailer.page_type()).into());
        }

        let buffer = cursor.into_inner();
        let crc = compute_crc(0, &buffer);
        if crc != trailer.crc() {
            return Err(NdbError::InvalidPageCrc(crc).into());
        }

        // rgentries
        let mut cursor = Cursor::new(buffer);
        let mut entries = Vec::with_capacity(entry_count);
        for _ in 0..entry_count {
            entries.push(<Self::Entry as BTreeEntry>::read(&mut cursor)?);
        }

        Ok(<Self as BTreePageReadWrite>::new(level, &entries, trailer)?)
    }

    fn write(&self, f: &mut dyn Write) -> io::Result<()> {
        let mut cursor = Cursor::new([0_u8; 500]);

        // rgentries
        let entries = self.entries();
        for entry in entries.iter().take(Self::MAX_BTREE_ENTRIES) {
            <Self::Entry as BTreeEntry>::write(entry, &mut cursor)?;
        }
        if entries.len() < Self::MAX_BTREE_ENTRIES {
            let entry = Default::default();
            for _ in entries.len()..Self::MAX_BTREE_ENTRIES {
                <Self::Entry as BTreeEntry>::write(&entry, &mut cursor)?;
            }
        }

        // cEnt
        cursor.write_u8(entries.len() as u8)?;

        // cEntMax
        cursor.write_u8(Self::MAX_BTREE_ENTRIES as u8)?;

        // cbEnt
        cursor.write_u8(Entry::ENTRY_SIZE as u8)?;

        // cLevel
        cursor.write_u8(self.level())?;

        let buffer = cursor.into_inner();
        let crc = compute_crc(0, &buffer);

        f.write_all(&buffer)?;

        // pageTrailer
        let trailer = self.trailer();
        let trailer = AnsiPageTrailer::new(
            trailer.page_type(),
            trailer.signature(),
            trailer.block_id(),
            crc,
        );
        trailer.write(f)
    }
}

pub trait BlockTrailerReadWrite: BlockTrailer + Copy + Sized {
    const SIZE: u16;

    fn new(size: u16, signature: u16, crc: u32, block_id: Self::BlockId) -> NdbResult<Self>;
    fn read(f: &mut dyn Read) -> io::Result<Self>;
    fn write(&self, f: &mut dyn Write) -> io::Result<()>;
    fn verify_block_id(&self, is_internal: bool) -> NdbResult<()>;
}

pub trait BlockReadWrite: Block + Sized {
    fn new(encoding: NdbCryptMethod, data: Vec<u8>, trailer: Self::Trailer) -> NdbResult<Self>;

    fn read<R: Read + Seek>(f: &mut R, size: u16, encoding: NdbCryptMethod) -> io::Result<Self> {
        let mut data = vec![0; size as usize];
        f.read_exact(&mut data)?;

        let offset = i64::from(block_size(size) - size - Self::Trailer::SIZE);
        if offset > 0 {
            f.seek(SeekFrom::Current(offset))?;
        }

        let trailer = Self::Trailer::read(f)?;
        if trailer.size() != size {
            return Err(NdbError::InvalidBlockSize(trailer.size()).into());
        }
        trailer.verify_block_id(false)?;
        let crc = compute_crc(0, &data);
        if crc != trailer.crc() {
            return Err(NdbError::InvalidBlockCrc(crc).into());
        }

        match encoding {
            NdbCryptMethod::Cyclic => {
                let key = trailer.cyclic_key();
                cyclic::encode_decode_block(&mut data, key);
            }
            NdbCryptMethod::Permute => {
                permute::decode_block(&mut data);
            }
            _ => {}
        }

        Ok(Self::new(encoding, data, trailer)?)
    }

    fn write<W: Write + Seek>(&self, f: &mut W) -> io::Result<()> {
        let mut data = self.data().to_vec();
        let trailer = self.trailer();

        match self.encoding() {
            NdbCryptMethod::Cyclic => {
                let key = trailer.cyclic_key();
                cyclic::encode_decode_block(&mut data, key);
            }
            NdbCryptMethod::Permute => {
                permute::encode_block(&mut data);
            }
            _ => {}
        }

        let crc = compute_crc(0, &data);
        let trailer = Self::Trailer::new(
            data.len() as u16,
            trailer.signature(),
            crc,
            trailer.block_id(),
        )?;

        f.write_all(&data)?;

        let size = data.len() as u16;
        let offset = i64::from(block_size(size) - size - UnicodeBlockTrailer::SIZE);
        if offset > 0 {
            f.seek(SeekFrom::Current(offset))?;
        }

        trailer.write(f)
    }
}

pub trait IntermediateTreeHeaderReadWrite: IntermediateTreeHeader + Copy + Sized {
    const HEADER_SIZE: u16;

    fn read(f: &mut dyn Read) -> io::Result<Self>;
    fn write(&self, f: &mut dyn Write) -> io::Result<()>;
}

pub trait IntermediateTreeEntryReadWrite: Copy + Sized {
    const ENTRY_SIZE: u16;

    fn read(f: &mut dyn Read) -> io::Result<Self>;
    fn write(&self, f: &mut dyn Write) -> io::Result<()>;
}

pub trait IntermediateTreeBlockReadWrite: IntermediateTreeBlock + Sized {
    fn new(
        header: Self::Header,
        entries: Vec<Self::Entry>,
        trailer: Self::Trailer,
    ) -> NdbResult<Self>;

    fn read<R: Read + Seek>(f: &mut R, size: u16) -> io::Result<Self> {
        let mut data = vec![0; size as usize];
        f.read_exact(&mut data)?;
        let mut cursor = Cursor::new(data.as_slice());

        let header = Self::Header::read(&mut cursor)?;
        let entry_count = header.entry_count();

        if entry_count * Self::Entry::ENTRY_SIZE > size - Self::Header::HEADER_SIZE {
            return Err(NdbError::InvalidInternalBlockEntryCount(entry_count).into());
        }

        let entries = (0..entry_count)
            .map(move |_| <Self::Entry as IntermediateTreeEntryReadWrite>::read(&mut cursor))
            .collect::<io::Result<Vec<_>>>()?;

        let offset = Self::Header::HEADER_SIZE + entry_count * Self::Entry::ENTRY_SIZE;
        let offset = i64::from(block_size(size + Self::Trailer::SIZE) - offset);
        match offset.cmp(&0) {
            Ordering::Greater => {
                f.seek(SeekFrom::Current(offset))?;
            }
            Ordering::Less => return Err(NdbError::InvalidBlockSize(size).into()),
            _ => {}
        }

        let trailer = Self::Trailer::read(f)?;
        trailer.verify_block_id(true)?;

        let crc = compute_crc(0, &data);
        if crc != trailer.crc() {
            return Err(NdbError::InvalidBlockCrc(crc).into());
        }

        Ok(Self::new(header, entries, trailer)?)
    }

    fn write<W: Write + Seek>(&self, f: &mut W) -> io::Result<()> {
        let mut curor = Cursor::new(vec![
            0_u8;
            Self::Header::HEADER_SIZE as usize
                + self.entries().len()
                    * Self::Entry::ENTRY_SIZE as usize
        ]);

        self.header().write(&mut curor)?;
        for entry in self.entries() {
            entry.write(&mut curor)?;
        }

        let data = curor.into_inner();
        let trailer = self.trailer();
        let crc = compute_crc(0, &data);
        let trailer = Self::Trailer::new(
            data.len() as u16,
            trailer.signature(),
            crc,
            trailer.block_id(),
        )?;

        let offset = trailer.size() + Self::Trailer::SIZE;
        let offset = block_size(offset) - offset;

        f.write_all(&data)?;
        f.seek(SeekFrom::Current(i64::from(offset)))?;
        trailer.write(f)
    }
}
