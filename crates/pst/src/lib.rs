#![doc = include_str!("../README.md")]

use std::{
    fs::File,
    io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    mem,
    path::Path,
    sync::Mutex,
};
use thiserror::Error;

pub mod ltp;
pub mod messaging;
pub mod ndb;

mod block_sig;
mod crc;
mod encode;

use ndb::{
    block::*, block_id::*, block_ref::*, byte_index::*, header::*, page::*, read_write::*, root::*,
    *,
};

#[derive(Error, Debug)]
pub enum PstError {
    #[error("Cannot write to file: {0}")]
    NoWriteAccess(String),
    #[error("I/O error: {0:?}")]
    Io(#[from] io::Error),
    #[error("Failed to lock file")]
    LockError,
    #[error("Integer conversion failed")]
    IntegerConversion,
    #[error("Node Database error: {0}")]
    NodeDatabaseError(#[from] NdbError),
    #[error("AllocationMapPage not found: {0}")]
    AllocationMapPageNotFound(usize),
}

impl From<&PstError> for io::Error {
    fn from(err: &PstError) -> io::Error {
        match err {
            PstError::NoWriteAccess(path) => {
                io::Error::new(io::ErrorKind::PermissionDenied, path.as_str())
            }
            err => io::Error::other(format!("{err:?}")),
        }
    }
}

impl From<PstError> for io::Error {
    fn from(err: PstError) -> io::Error {
        match err {
            PstError::NoWriteAccess(path) => {
                io::Error::new(io::ErrorKind::PermissionDenied, path.as_str())
            }
            PstError::Io(err) => err,
            err => io::Error::other(err),
        }
    }
}

type PstResult<T> = std::result::Result<T, PstError>;

/// [PST File](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/6b57253b-0853-47bb-99bb-d4b8f78105f0)
pub trait PstFile: Sized
where
    u64: From<<Self::BlockId as BlockId>::Index> + From<<Self::ByteIndex as ByteIndex>::Index>,
{
    type BlockId: BlockId + BlockIdReadWrite;
    type ByteIndex: ByteIndex + ByteIndexReadWrite;
    type BlockRef: BlockRef<Block = Self::BlockId, Index = Self::ByteIndex> + BlockRefReadWrite;
    type Root: Root<BTreeRef = Self::BlockRef>;
    type Header: Header<Root = Self::Root>;
    type PageTrailer: PageTrailer<BlockId = Self::BlockId> + PageTrailerReadWrite;
    type BTreeKey: BTreeEntryKey;
    type NodeBTreeEntry: NodeBTreeEntry<Block = Self::BlockId> + BTreeEntry<Key = Self::BTreeKey>;
    type NodeBTree: NodeBTree<Self, Self::NodeBTreeEntry>;
    type BlockBTreeEntry: BlockBTreeEntry<Block = Self::BlockRef> + BTreeEntry<Key = Self::BTreeKey>;
    type BlockBTree: BlockBTree<Self, Self::BlockBTreeEntry>;
    type IntermediateDataTreeEntry: IntermediateTreeEntry;
    type BlockTrailer: BlockTrailer<BlockId = Self::BlockId>;
    type AllocationMapPage: AllocationMapPage<Self>;
    type AllocationPageMapPage: AllocationPageMapPage<Self>;
    type FreeMapPage: FreeMapPage<Self>;
    type FreePageMapPage: FreePageMapPage<Self>;
    type DensityListPage: DensityListPage<Self>;

    fn reader(&self) -> &Mutex<BufReader<File>>;
    fn writer(&mut self) -> &PstResult<Mutex<BufWriter<File>>>;
    fn header(&self) -> &Self::Header;
    fn header_mut(&mut self) -> &mut Self::Header;
    fn density_list(&self) -> Result<&dyn DensityListPage<Self>, &io::Error>;
    fn rebuild_allocation_map(&mut self) -> io::Result<()>;
}

pub struct UnicodePstFile {
    reader: Mutex<BufReader<File>>,
    writer: PstResult<Mutex<BufWriter<File>>>,
    header: UnicodeHeader,
    density_list: io::Result<UnicodeDensityListPage>,
}

impl UnicodePstFile {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let writer = File::create(&path)
            .map(BufWriter::new)
            .map(Mutex::new)
            .map_err(|_| PstError::NoWriteAccess(path.as_ref().display().to_string()));
        let mut reader = BufReader::new(File::open(path)?);
        reader.seek(SeekFrom::Start(0))?;
        let header = UnicodeHeader::read(&mut reader)?;
        let density_list = UnicodeDensityListPage::read(&mut reader);

        Ok(Self {
            reader: Mutex::new(reader),
            writer,
            header,
            density_list,
        })
    }
}

impl PstFile for UnicodePstFile {
    type BlockId = UnicodeBlockId;
    type ByteIndex = UnicodeByteIndex;
    type BlockRef = UnicodeBlockRef;
    type Root = UnicodeRoot;
    type Header = UnicodeHeader;
    type PageTrailer = UnicodePageTrailer;
    type BTreeKey = u64;
    type NodeBTreeEntry = UnicodeNodeBTreeEntry;
    type NodeBTree = UnicodeNodeBTree;
    type BlockBTreeEntry = UnicodeBlockBTreeEntry;
    type BlockBTree = UnicodeBlockBTree;
    type IntermediateDataTreeEntry = UnicodeDataTreeEntry;
    type BlockTrailer = UnicodeBlockTrailer;
    type AllocationMapPage = UnicodeMapPage<{ PageType::AllocationMap as u8 }>;
    type AllocationPageMapPage = UnicodeMapPage<{ PageType::AllocationPageMap as u8 }>;
    type FreeMapPage = UnicodeMapPage<{ PageType::FreeMap as u8 }>;
    type FreePageMapPage = UnicodeMapPage<{ PageType::FreePageMap as u8 }>;
    type DensityListPage = UnicodeDensityListPage;

    fn reader(&self) -> &Mutex<BufReader<File>> {
        &self.reader
    }

    fn writer(&mut self) -> &PstResult<Mutex<BufWriter<File>>> {
        &self.writer
    }

    fn header(&self) -> &Self::Header {
        &self.header
    }

    fn header_mut(&mut self) -> &mut Self::Header {
        &mut self.header
    }

    fn density_list(&self) -> Result<&dyn DensityListPage<Self>, &io::Error> {
        self.density_list.as_ref().map(|dl| dl as _)
    }

    fn rebuild_allocation_map(&mut self) -> io::Result<()> {
        <Self as PstFileReadWrite>::rebuild_allocation_map(self)
    }
}

pub struct AnsiPstFile {
    reader: Mutex<BufReader<File>>,
    writer: PstResult<Mutex<BufWriter<File>>>,
    header: ndb::header::AnsiHeader,
    density_list: io::Result<ndb::page::AnsiDensityListPage>,
}

impl AnsiPstFile {
    pub fn read(path: impl AsRef<Path>) -> io::Result<Self> {
        let writer = File::create(&path)
            .map(BufWriter::new)
            .map(Mutex::new)
            .map_err(|_| PstError::NoWriteAccess(path.as_ref().display().to_string()));
        let mut reader = BufReader::new(File::open(path)?);
        let header = AnsiHeader::read(&mut reader)?;
        let density_list = AnsiDensityListPage::read(&mut reader);
        Ok(Self {
            reader: Mutex::new(reader),
            writer,
            header,
            density_list,
        })
    }
}

impl PstFile for AnsiPstFile {
    type BlockId = AnsiBlockId;
    type ByteIndex = AnsiByteIndex;
    type BlockRef = AnsiBlockRef;
    type Root = AnsiRoot;
    type Header = AnsiHeader;
    type PageTrailer = AnsiPageTrailer;
    type BTreeKey = u32;
    type NodeBTreeEntry = AnsiNodeBTreeEntry;
    type NodeBTree = AnsiNodeBTree;
    type BlockBTreeEntry = AnsiBlockBTreeEntry;
    type BlockBTree = AnsiBlockBTree;
    type IntermediateDataTreeEntry = AnsiDataTreeEntry;
    type BlockTrailer = AnsiBlockTrailer;
    type AllocationMapPage = AnsiMapPage<{ PageType::AllocationMap as u8 }>;
    type AllocationPageMapPage = AnsiMapPage<{ PageType::AllocationPageMap as u8 }>;
    type FreeMapPage = AnsiMapPage<{ PageType::FreeMap as u8 }>;
    type FreePageMapPage = AnsiMapPage<{ PageType::FreePageMap as u8 }>;
    type DensityListPage = AnsiDensityListPage;

    fn reader(&self) -> &Mutex<BufReader<File>> {
        &self.reader
    }

    fn writer(&mut self) -> &PstResult<Mutex<BufWriter<File>>> {
        &self.writer
    }

    fn header(&self) -> &Self::Header {
        &self.header
    }

    fn header_mut(&mut self) -> &mut Self::Header {
        &mut self.header
    }

    fn density_list(&self) -> Result<&dyn DensityListPage<Self>, &io::Error> {
        self.density_list.as_ref().map(|dl| dl as _)
    }

    fn rebuild_allocation_map(&mut self) -> io::Result<()> {
        <Self as PstFileReadWrite>::rebuild_allocation_map(self)
    }
}

const AMAP_FIRST_OFFSET: u64 = 0x4400;
const AMAP_DATA_SIZE: u64 = size_of::<MapBits>() as u64 * 8 * 64;

const PMAP_FIRST_OFFSET: u64 = AMAP_FIRST_OFFSET + PAGE_SIZE as u64;
const PMAP_PAGE_COUNT: u64 = 8;
const PMAP_DATA_SIZE: u64 = AMAP_DATA_SIZE * PMAP_PAGE_COUNT;

const FMAP_FIRST_SIZE: u64 = 128;
const FMAP_FIRST_DATA_SIZE: u64 = AMAP_DATA_SIZE * FMAP_FIRST_SIZE;
const FMAP_FIRST_OFFSET: u64 = AMAP_FIRST_OFFSET + FMAP_FIRST_DATA_SIZE + (2 * PAGE_SIZE) as u64;
const FMAP_PAGE_COUNT: u64 = size_of::<MapBits>() as u64;
const FMAP_DATA_SIZE: u64 = AMAP_DATA_SIZE * FMAP_PAGE_COUNT;

const FPMAP_FIRST_SIZE: u64 = 128 * 64;
const FPMAP_FIRST_DATA_SIZE: u64 = AMAP_DATA_SIZE * FPMAP_FIRST_SIZE;
const FPMAP_FIRST_OFFSET: u64 = AMAP_FIRST_OFFSET + FPMAP_FIRST_DATA_SIZE + (3 * PAGE_SIZE) as u64;
const FPMAP_PAGE_COUNT: u64 = size_of::<MapBits>() as u64 * 64;
const FPMAP_DATA_SIZE: u64 = AMAP_DATA_SIZE * FPMAP_PAGE_COUNT;

struct AllocationMapPageInfo<Pst>
where
    Pst: PstFile,
    <Pst as PstFile>::AllocationMapPage: AllocationMapPageReadWrite<Pst>,
    u64: From<<<Pst as PstFile>::BlockId as BlockId>::Index>
        + From<<<Pst as PstFile>::ByteIndex as ByteIndex>::Index>,
{
    amap_page: <Pst as PstFile>::AllocationMapPage,
    free_space: u64,
}

trait PstFileReadWrite: PstFile
where
    <Self as PstFile>::BlockId:
        From<<<Self as PstFile>::ByteIndex as ByteIndex>::Index> + BlockIdReadWrite,
    <Self as PstFile>::ByteIndex: ByteIndex<Index: TryFrom<u64>> + ByteIndexReadWrite,
    <Self as PstFile>::BlockRef: BlockRefReadWrite,
    <Self as PstFile>::Root: RootReadWrite,
    <Self as PstFile>::Header: HeaderReadWrite,
    <Self as PstFile>::PageTrailer: PageTrailerReadWrite,
    <Self as PstFile>::BTreeKey: BTreePageKeyReadWrite,
    <Self as PstFile>::NodeBTreeEntry: NodeBTreeEntryReadWrite,
    <Self as PstFile>::NodeBTree: RootBTreeReadWrite,
    <<Self as PstFile>::NodeBTree as RootBTree>::IntermediatePage:
        RootBTreeIntermediatePageReadWrite<
            Self,
            <Self as PstFile>::NodeBTreeEntry,
            <<Self as PstFile>::NodeBTree as RootBTree>::LeafPage,
        >,
    <<Self as PstFile>::NodeBTree as RootBTree>::LeafPage: RootBTreeLeafPageReadWrite<Self>,
    <Self as PstFile>::BlockBTreeEntry: BlockBTreeEntryReadWrite,
    <Self as PstFile>::BlockBTree: RootBTreeReadWrite,

    <<Self as PstFile>::BlockBTree as RootBTree>::IntermediatePage:
        RootBTreeIntermediatePageReadWrite<
            Self,
            <Self as PstFile>::BlockBTreeEntry,
            <<Self as PstFile>::BlockBTree as RootBTree>::LeafPage,
        >,
    <<Self as PstFile>::BlockBTree as RootBTree>::LeafPage: RootBTreeLeafPageReadWrite<Self>,
    <Self as PstFile>::AllocationMapPage: AllocationMapPageReadWrite<Self>,
    <Self as PstFile>::AllocationPageMapPage: AllocationPageMapPageReadWrite<Self>,
    <Self as PstFile>::FreeMapPage: FreeMapPageReadWrite<Self>,
    <Self as PstFile>::FreePageMapPage: FreePageMapPageReadWrite<Self>,
    <Self as PstFile>::DensityListPage: DensityListPageReadWrite<Self>,
    u64: From<<<Self as PstFile>::BlockId as BlockId>::Index>
        + From<<<Self as PstFile>::ByteIndex as ByteIndex>::Index>,
{
    /// [Crash Recovery and AMap Rebuilding](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/d9bcc1fd-c66a-41b3-b6d7-ed09d2a25ced)
    fn rebuild_allocation_map(&mut self) -> io::Result<()> {
        let header = self.header();
        let root = header.root();
        if AmapStatus::Invalid != root.amap_is_valid() {
            return Ok(());
        }

        let num_amap_pages = u64::from(root.file_eof_index().index()) - AMAP_FIRST_OFFSET;
        let num_amap_pages = (num_amap_pages + AMAP_DATA_SIZE - 1) / AMAP_DATA_SIZE;

        let mut amap_pages: Vec<_> = (0..num_amap_pages)
            .map(|index| {
                let has_pmap_page = index % 8 == 0;
                let has_fmap_page = has_pmap_page
                    && index >= FMAP_FIRST_SIZE
                    && (index - FMAP_FIRST_SIZE) % FMAP_PAGE_COUNT == 0;
                let has_fpmap_page = has_pmap_page
                    && index >= FPMAP_FIRST_SIZE
                    && (index - FPMAP_FIRST_SIZE) % FPMAP_PAGE_COUNT == 0;

                let index =
                    <<<Self as PstFile>::ByteIndex as ByteIndex>::Index as TryFrom<u64>>::try_from(
                        index * AMAP_DATA_SIZE + AMAP_FIRST_OFFSET,
                    )
                    .map_err(|_| PstError::IntegerConversion)?;
                let index = <<Self as PstFile>::BlockRef as BlockRef>::Block::from(index);

                let trailer = <<Self as PstFile>::PageTrailer as PageTrailerReadWrite>::new(
                    PageType::AllocationMap,
                    0,
                    index,
                    0,
                );

                let mut map_bits = [0; mem::size_of::<MapBits>()];
                let mut reserved = 1;
                if has_pmap_page {
                    reserved += 1;
                }
                if has_fmap_page {
                    reserved += 1;
                }
                if has_fpmap_page {
                    reserved += 1;
                }

                let free_space = AMAP_DATA_SIZE - (reserved * PAGE_SIZE) as u64;

                let reserved = &[0xFF; 4][..reserved];
                map_bits[..reserved.len()].copy_from_slice(&reserved);

                let amap_page =
                    <<Self as PstFile>::AllocationMapPage as AllocationMapPageReadWrite<Self>>::new(
                        map_bits, trailer,
                    )?;
                Ok(AllocationMapPageInfo::<Self> {
                    amap_page,
                    free_space,
                })
            })
            .collect::<PstResult<Vec<_>>>()?;

        {
            let mut reader = self.reader().lock().map_err(|_| PstError::LockError)?;
            let reader = &mut *reader;

            let node_btree =
                <Self::NodeBTree as RootBTreeReadWrite>::read(reader, *root.node_btree())?;

            let block_btree =
                <Self::BlockBTree as RootBTreeReadWrite>::read(reader, *root.block_btree())?;

            self.mark_node_btree_allocations(reader, &node_btree, &block_btree, &mut amap_pages)?;
        }

        let free_bytes = amap_pages.iter().map(|page| page.free_space).sum();

        let pmap_pages: Vec<_> = (0..(num_amap_pages / 8))
            .map(|index| {
                let index =
                    <<<Self as PstFile>::ByteIndex as ByteIndex>::Index as TryFrom<u64>>::try_from(
                        index * PMAP_DATA_SIZE + PMAP_FIRST_OFFSET,
                    )
                    .map_err(|_| PstError::IntegerConversion)?;
                let index = <<Self as PstFile>::BlockRef as BlockRef>::Block::from(index);

                let trailer = <<Self as PstFile>::PageTrailer as PageTrailerReadWrite>::new(
                    PageType::AllocationPageMap,
                    0,
                    index,
                    0,
                );

                let map_bits = [0xFF; mem::size_of::<MapBits>()];

                let pmap_page =
                    <<Self as PstFile>::AllocationPageMapPage as AllocationPageMapPageReadWrite<
                        Self,
                    >>::new(map_bits, trailer)?;
                Ok(pmap_page)
            })
            .collect::<PstResult<Vec<_>>>()?;

        let fmap_pages: Vec<_> = (0..((num_amap_pages.max(FMAP_FIRST_SIZE) - FMAP_FIRST_SIZE)
            / FMAP_PAGE_COUNT))
            .map(|index| {
                let index =
                    <<<Self as PstFile>::ByteIndex as ByteIndex>::Index as TryFrom<u64>>::try_from(
                        index * FMAP_DATA_SIZE + FMAP_FIRST_OFFSET,
                    )
                    .map_err(|_| PstError::IntegerConversion)?;
                let index = <<Self as PstFile>::BlockRef as BlockRef>::Block::from(index);

                let trailer = <<Self as PstFile>::PageTrailer as PageTrailerReadWrite>::new(
                    PageType::FreeMap,
                    0,
                    index,
                    0,
                );

                let map_bits = [0; mem::size_of::<MapBits>()];

                let fmap_page =
                    <<Self as PstFile>::FreeMapPage as FreeMapPageReadWrite<Self>>::new(
                        map_bits, trailer,
                    )?;
                Ok(fmap_page)
            })
            .collect::<PstResult<Vec<_>>>()?;

        let fpmap_pages: Vec<_> = (0..((num_amap_pages.max(FPMAP_FIRST_SIZE) - FPMAP_FIRST_SIZE)
            / FPMAP_PAGE_COUNT))
            .map(|index| {
                let index =
                    <<<Self as PstFile>::ByteIndex as ByteIndex>::Index as TryFrom<u64>>::try_from(
                        index * FPMAP_DATA_SIZE + FPMAP_FIRST_OFFSET,
                    )
                    .map_err(|_| PstError::IntegerConversion)?;
                let index = <<Self as PstFile>::BlockRef as BlockRef>::Block::from(index);

                let trailer = <<Self as PstFile>::PageTrailer as PageTrailerReadWrite>::new(
                    PageType::FreePageMap,
                    0,
                    index,
                    0,
                );

                let map_bits = [0; mem::size_of::<MapBits>()];

                let fmap_page = <<Self as PstFile>::FreePageMapPage as FreePageMapPageReadWrite<
                    Self,
                >>::new(map_bits, trailer)?;
                Ok(fmap_page)
            })
            .collect::<PstResult<Vec<_>>>()?;

        let mut header = header.clone();
        header.root_mut().reset_free_size(free_bytes)?;

        let mut writer = self
            .writer()
            .as_ref()?
            .lock()
            .map_err(|_| PstError::LockError)?;
        let writer = &mut *writer;

        for page in amap_pages.into_iter().map(|info| info.amap_page) {
            let index: <<Self as PstFile>::BlockId as BlockId>::Index =
                page.trailer().block_id().into();
            let index = u64::from(index);

            writer.seek(SeekFrom::Start(index))?;
            <Self::AllocationMapPage as AllocationMapPageReadWrite<Self>>::write(&page, writer)?;
        }

        for page in pmap_pages.into_iter() {
            let index: <<Self as PstFile>::BlockId as BlockId>::Index =
                page.trailer().block_id().into();
            let index = u64::from(index);

            writer.seek(SeekFrom::Start(index))?;
            <Self::AllocationPageMapPage as AllocationPageMapPageReadWrite<Self>>::write(
                &page, writer,
            )?;
        }

        for page in fmap_pages.into_iter() {
            let index: <<Self as PstFile>::BlockId as BlockId>::Index =
                page.trailer().block_id().into();
            let index = u64::from(index);

            writer.seek(SeekFrom::Start(index))?;
            <Self::FreeMapPage as FreeMapPageReadWrite<Self>>::write(&page, writer)?;
        }

        for page in fpmap_pages.into_iter() {
            let index: <<Self as PstFile>::BlockId as BlockId>::Index =
                page.trailer().block_id().into();
            let index = u64::from(index);

            writer.seek(SeekFrom::Start(index))?;
            <Self::FreePageMapPage as FreePageMapPageReadWrite<Self>>::write(&page, writer)?;
        }

        writer.flush()?;

        header.write(writer)?;
        writer.flush()?;

        Ok(())
    }

    fn mark_node_btree_allocations<R: Read + Seek>(
        &self,
        reader: &mut R,
        node_btree: &RootBTreePage<
            Self,
            <<Self as PstFile>::NodeBTree as RootBTree>::Entry,
            <<Self as PstFile>::NodeBTree as RootBTree>::IntermediatePage,
            <<Self as PstFile>::NodeBTree as RootBTree>::LeafPage,
        >,
        block_btree: &RootBTreePage<
            Self,
            <<Self as PstFile>::BlockBTree as RootBTree>::Entry,
            <<Self as PstFile>::BlockBTree as RootBTree>::IntermediatePage,
            <<Self as PstFile>::BlockBTree as RootBTree>::LeafPage,
        >,
        amap_pages: &mut Vec<AllocationMapPageInfo<Self>>,
    ) -> io::Result<()> {
        match node_btree {
            RootBTreePage::Intermediate(page, ..) => {
                let block_id = page.trailer().block_id();
                let index: <<Self as PstFile>::BlockId as BlockId>::Index = block_id.into();
                Self::mark_page_allocation(u64::from(index), amap_pages)?;

                for entry in page.entries() {
                    let node_btree =
                        <Self::NodeBTree as RootBTreeReadWrite>::read(reader, entry.block())?;
                    self.mark_node_btree_allocations(reader, &node_btree, block_btree, amap_pages)?;
                }
            }
            RootBTreePage::Leaf(page) => {
                let block_id = page.trailer().block_id();
                let index: <<Self as PstFile>::BlockId as BlockId>::Index = block_id.into();
                Self::mark_page_allocation(u64::from(index), amap_pages)?;
            }
        }
        Ok(())
    }

    fn mark_page_allocation(
        index: u64,
        amap_pages: &mut Vec<AllocationMapPageInfo<Self>>,
    ) -> io::Result<()> {
        let index = u64::from(index) - AMAP_FIRST_OFFSET;
        let amap_index =
            usize::try_from(index / AMAP_DATA_SIZE).map_err(|_| PstError::IntegerConversion)?;
        let entry = amap_pages
            .get_mut(amap_index)
            .ok_or(PstError::AllocationMapPageNotFound(amap_index))?;
        entry.free_space -= PAGE_SIZE as u64;

        let bytes = entry.amap_page.map_bits_mut();

        let bit_index = usize::try_from((index % AMAP_DATA_SIZE) / 64)
            .map_err(|_| PstError::IntegerConversion)?;
        let byte_index = bit_index / 8;
        let bit_index = bit_index % 8;

        if bit_index == 0 {
            bytes[byte_index] = 0xFF;
        } else {
            let mask = 0x00FF_u16 << bit_index;
            bytes[byte_index] |= (mask & 0xFF) as u8;
            bytes[byte_index + 1] |= ((mask >> 8) & 0xFF) as u8;
        }

        Ok(())
    }
}

impl PstFileReadWrite for UnicodePstFile {}
impl PstFileReadWrite for AnsiPstFile {}
