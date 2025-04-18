//! ## [Lists, Tables, and Properties (LTP) Layer](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/4c24c7d2-5c5a-4b99-88b2-f4b84cc293ae)

use std::io;
use thiserror::Error;

pub mod heap;
pub mod prop_context;
pub mod prop_type;
pub mod table_context;
pub mod tree;

pub(crate) mod read_write;

#[derive(Error, Debug)]
pub enum LtpError {
    #[error("Node Database error: {0}")]
    NodeDatabaseError(#[from] crate::ndb::NdbError),
    #[error("Invalid HID hidIndex: 0x{0:04X}")]
    InvalidHeapIndex(u16),
    #[error("Invalid HID hidType: {0:?}")]
    InvalidNodeType(crate::ndb::node_id::NodeIdType),
    #[error("Invalid HNHDR bSig: 0x{0:02X}")]
    InvalidHeapNodeSignature(u8),
    #[error("Invalid HNHDR bClientSig: 0x{0:02X}")]
    InvalidHeapNodeTypeSignature(u8),
    #[error("Invalid HNHDR rgbFillLevel: 0x{0:02X}")]
    InvalidHeapFillLevel(u8),
    #[error("HNPAGEMAP is out of space")]
    HeapPageOutOfSpace,
    #[error("Empty HNPAGEMAP rgibAlloc")]
    EmptyHeapPageAlloc,
    #[error("Invalid HNPAGEMAP rgibAlloc entry: 0x{0:04X}")]
    InvalidHeapPageAllocOffset(u16),
    #[error("Invalid HNPAGEMAP cAlloc: 0x{0:04X}")]
    InvalidHeapPageAllocCount(u16),
    #[error("Invalid HNPAGEMAP cFree: 0x{0:04X}")]
    InvalidHeapPageFreeCount(u16),
    #[error("Invalid BTHHEADER bType: {0:?}")]
    InvalidHeapTreeNodeType(heap::HeapNodeType),
    #[error("Invalid BTHHEADER cbKey: 0x{0:02X}")]
    InvalidHeapTreeKeySize(u8),
    #[error("Invalid BTHHEADER cbEnt: 0x{0:02X}")]
    InvalidHeapTreeDataSize(u8),
    #[error("Missing HID hidBlockIndex: {0}")]
    HeapBlockIndexNotFound(u16),
    #[error("Missing HID hidIndex: {0}")]
    HeapAllocIndexNotFound(u16),
    #[error("Invalid PC BTH Record wPropType: 0x{0:04X}")]
    InvalidPropertyType(u16),
    #[error("Invalid variable length PC value property type: {0:?}")]
    InvalidVariableLengthPropertyType(prop_type::PropertyType),
    #[error("Invalid multi-value property offset: 0x{0:X}")]
    InvalidMultiValuePropertyOffset(usize),
    #[error("Invalid multi-value property count: 0x{0:X}")]
    InvalidMultiValuePropertyCount(usize),
    #[error("Missing PC sub-node value: 0x{0:08X}")]
    PropertySubNodeValueNotFound(u32),
    #[error("Invalid small PC value property type: {0:?}")]
    InvalidSmallPropertyType(prop_type::PropertyType),
    #[error("Invalid PC property tree key size: 0x{0:X}")]
    InvalidPropertyTreeKeySize(u8),
    #[error("Invalid PC property tree entry size: 0x{0:X}")]
    InvalidPropertyTreeEntrySize(u8),
    #[error("Invalid TCINFO bType: {0:?}")]
    InvalidTableContextHeapTreeNodeType(heap::HeapNodeType),
    #[error("Invalid TCOLDESC count: 0x{0:X}")]
    InvalidTableContextColumnCount(usize),
    #[error("Invalid TCINFO rgib[TCI_4b]: 0x{0:04X}")]
    InvalidTableContext4ByteOffset(u16),
    #[error("Invalid TCINFO rgib[TCI_2b]: 0x{0:04X}")]
    InvalidTableContext2ByteOffset(u16),
    #[error("Invalid TCINFO rgib[TCI_1b]: 0x{0:04X}")]
    InvalidTableContext1ByteOffset(u16),
    #[error("Invalid TCINFO rgib[TCI_bm]: 0x{0:04X}")]
    InvalidTableContextBitmaskOffset(u16),
    #[error("Missing PidTagLtpRowId in TCINFO rgTCOLDESC[0]")]
    TableContextRowIdColumnNotFound,
    #[error("Invalid TCINFO rgTCOLDESC[0]: PropId: 0x{0:04X}, PropType: {1:?}")]
    InvalidTableContextRowIdColumn(u16, prop_type::PropertyType),
    #[error("Missing PidTagLtpRowVer in TCINFO rgTCOLDESC[1]")]
    TableContextRowVersionColumnNotFound,
    #[error("Invalid TCINFO rgTCOLDESC[1]: PropId: 0x{0:04X}, PropType: {1:?}")]
    InvalidTableContextRowVersionColumn(u16, prop_type::PropertyType),
    #[error("Invalid TCOLDESC property type: {0:?}")]
    InvalidTableColumnPropertyType(prop_type::PropertyType),
    #[error("Invalid TCOLDESC ibData: 0x{0:04X}")]
    InvalidTableColumnOffset(u16),
    #[error("Invalid TCOLDESC cbData: 0x{0:04X}")]
    InvalidTableColumnSize(u8),
    #[error("Invalid TCOLDESC iBit: 0x{0:04X}")]
    InvalidTableColumnBitmaskOffset(u8),
    #[error("Invalid TCOLDESC BOOL value: 0x{0:02X}")]
    InvalidTableColumnBooleanValue(u8),
    #[error("Missing TCROWID: 0x{0:08X}")]
    TableRowIdNotFound(u32),
}

impl From<LtpError> for io::Error {
    fn from(err: LtpError) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, err)
    }
}

pub type LtpResult<T> = Result<T, LtpError>;
