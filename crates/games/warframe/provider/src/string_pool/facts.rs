//! String-pool layout and executable reference used by [`super`].

use provider_sdk::memory::Rva;

#[derive(Debug, ..Copy, ..Eq)]
pub(crate) struct StringPoolFacts {
    /// Global storage holding the string-pool pointer.
    pub(crate) root: Rva,
    /// Checks the root reference and token lookup: low 16 bits select a 16-byte
    /// bucket entry; high 16 bits are a byte offset within the pointed-to bucket.
    pub(crate) decoder: Rva,
}

pub(crate) const STRINGS: StringPoolFacts = StringPoolFacts {
    root: Rva::new(0x028a_39a0),
    decoder: Rva::new(0x0030_9440),
};
