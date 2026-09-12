//! Address-bound scalar encoding used by inventory counts and account balances.

use provider_sdk::memory::ObjectOffset;

/// Record-relative offsets for two u32 words: `check = stored ^ check_xor`.
/// `value = rotate_left(stored) XOR low32(stored_address >> address_shift) XOR value_xor`.
#[derive(Debug, ..Copy, ..Eq)]
pub(crate) struct AddressEncodedScalarFacts {
    pub(crate) check: ObjectOffset,
    pub(crate) stored: ObjectOffset,
    pub(crate) rotate_left: u32,
    pub(crate) address_shift: u32,
    pub(crate) value_xor: u32,
    pub(crate) check_xor: u32,
}

impl AddressEncodedScalarFacts {
    /// The address belongs to the stored word in game memory, not the local buffer.
    /// Signedness is interpreted by the topic after decoding the raw bits.
    pub(crate) fn decode(self, check: u32, stored: u32, stored_address: u64) -> Option<u32> {
        if self.rotate_left >= u32::BITS || self.address_shift >= u64::BITS {
            return None;
        }
        let [a, b, c, d, ..] = (stored_address >> self.address_shift).to_le_bytes();
        (check == stored ^ self.check_xor).then(|| {
            stored.rotate_left(self.rotate_left) ^ u32::from_le_bytes([a, b, c, d]) ^ self.value_xor
        })
    }
}
