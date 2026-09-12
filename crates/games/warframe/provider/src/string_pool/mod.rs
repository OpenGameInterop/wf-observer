//! Resolves game string tokens using the layout in [`facts`].

pub(crate) mod facts;

use std::{collections::BTreeMap, mem::size_of};

use memory_reader::ProcessMemory;
use provider_sdk::memory::{ReadError, TargetReader};

use facts::StringPoolFacts;

const MAX_STRING_TOKENS: usize = 16_384;
const MAX_STRING_BYTES: usize = 255;
const MAX_DECODED_BYTES: usize = 1024 * 1024;
const STRING_READ_BYTES: usize = 64;

/// Process-scoped cache for immutable game string tokens.
#[derive(Default)]
pub(crate) struct StringTokenCache {
    values: BTreeMap<(u64, u32), String>,
    decoded_bytes: usize,
}

impl StringTokenCache {
    pub(crate) fn pool(
        reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
        facts: StringPoolFacts,
    ) -> Result<u64, ReadError> {
        let root = reader.module_address(facts.root, size_of::<u64>())?;
        let pool = reader.read_u64(root)?;
        require_aligned(pool, 8, "root")?;
        Ok(pool)
    }

    pub(crate) fn resolve(
        &mut self,
        reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
        pool: u64,
        token: u32,
    ) -> Result<String, ReadError> {
        let key = (pool, token);
        if let Some(value) = self.values.get(&key) {
            return Ok(value.clone());
        }
        let bucket_slot = pool
            .checked_add(u64::from(token & 0xffff) * 16)
            .ok_or(ReadError::overflow("string token address"))?;
        let bucket = reader.read_u64(bucket_slot)?;
        require_aligned(bucket, 8, "token bucket")?;
        let address = bucket
            .checked_add(u64::from(token >> 16))
            .ok_or(ReadError::overflow("string token address"))?;
        let value = read_c_string(reader, address)?;
        if self.values.len() < MAX_STRING_TOKENS
            && self.decoded_bytes + value.len() <= MAX_DECODED_BYTES
        {
            self.decoded_bytes += value.len();
            self.values.insert(key, value.clone());
        }
        Ok(value)
    }
}

/// Validates the decoder instructions that define the pool lookup algorithm.
pub(crate) fn validate_string_pool_layout(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    facts: StringPoolFacts,
) -> Result<(), ReadError> {
    let mut code = [0_u8; 37];
    reader.read_module(facts.decoder, &mut code)?;
    let root_reference = facts
        .decoder
        .checked_add(12)
        .ok_or(ReadError::overflow("string-pool decoder"))?;
    if code[..12]
        != [
            0x40, 0x53, 0x48, 0x83, 0xec, 0x20, 0x44, 0x8b, 0x01, 0x48, 0x8b, 0xda,
        ]
        || code[12..15] != [0x48, 0x8b, 0x05]
        || root_reference.rip_target(7, &code[15..19]) != Some(facts.root)
        || code[19..]
            != [
                0x41, 0x0f, 0xb7, 0xc8, 0x48, 0x03, 0xc9, 0x49, 0xc1, 0xe8, 0x10, 0x48, 0x8b, 0x0c,
                0xc8, 0x49, 0x03, 0xc8,
            ]
    {
        return Err(ReadError::layout("string-pool decoder"));
    }
    Ok(())
}

fn read_c_string(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    address: u64,
) -> Result<String, ReadError> {
    let mut value = Vec::with_capacity(STRING_READ_BYTES);
    while value.len() <= MAX_STRING_BYTES {
        let remaining = MAX_STRING_BYTES + 1 - value.len();
        let chunk_address = address
            .checked_add(
                u64::try_from(value.len())
                    .map_err(|_| ReadError::overflow("string-pool string address"))?,
            )
            .ok_or(ReadError::overflow("string-pool string address"))?;
        // Aligned chunks cannot cross a page after a string's terminating NUL.
        let offset = usize::try_from(chunk_address % STRING_READ_BYTES as u64)
            .map_err(|_| ReadError::overflow("string-pool string address"))?;
        let boundary = STRING_READ_BYTES - offset;
        let read_bytes = remaining.min(boundary);
        let mut chunk = [0_u8; STRING_READ_BYTES];
        reader.read_at(chunk_address, &mut chunk[..read_bytes])?;
        if let Some(nul) = chunk[..read_bytes].iter().position(|byte| *byte == 0) {
            value.extend_from_slice(&chunk[..nul]);
            if value.len() > MAX_STRING_BYTES || value.iter().any(|byte| !byte.is_ascii_graphic()) {
                return Err(ReadError::invalid("string-pool string"));
            }
            return String::from_utf8(value).map_err(|_| ReadError::invalid("string-pool string"));
        }
        value.extend_from_slice(&chunk[..read_bytes]);
    }
    Err(ReadError::limit("string-pool string length"))
}

fn require_aligned(address: u64, alignment: u64, location: &'static str) -> Result<(), ReadError> {
    if address != 0 && address.is_multiple_of(alignment) {
        Ok(())
    } else {
        Err(ReadError::invalid(location))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target::READ_LIMITS;
    use memory_reader::{AccessError, MemoryModule, Target};

    struct Token;

    impl ProcessMemory for Token {
        fn target(&self) -> &Target {
            unreachable!("token-only test")
        }
        fn verify(&mut self) -> Result<(), AccessError> {
            unreachable!("token-only test")
        }
        fn modules(&mut self) -> Result<Vec<MemoryModule>, AccessError> {
            unreachable!("token-only test")
        }
        fn read_into(&mut self, address: u64, output: &mut [u8]) -> Result<(), AccessError> {
            match (address, output.len()) {
                (0x10010, 8) => output.copy_from_slice(&0x20000_u64.to_le_bytes()),
                (0x20000, STRING_READ_BYTES) => {
                    output.fill(0);
                    output[..4].copy_from_slice(b"Item");
                }
                (0x20ffc, 4) => output.copy_from_slice(b"End\0"),
                _ => {
                    return Err(AccessError::InvalidReadRange {
                        address,
                        length: output.len(),
                    });
                }
            }
            Ok(())
        }
    }

    #[test]
    fn pool_strings_can_end_at_a_page_boundary() -> Result<(), ReadError> {
        let mut memory = Token;
        let mut reader = TargetReader::new(&mut memory, 0x10000, 0x20000, READ_LIMITS)?;
        assert_eq!(read_c_string(&mut reader, 0x20ffc)?, "End");
        Ok(())
    }

    #[test]
    fn full_caches_still_resolve_uncached_tokens() -> Result<(), ReadError> {
        for byte_limit in [false, true] {
            let mut cache = StringTokenCache::default();
            if byte_limit {
                cache.values.insert((0, 0), "x".repeat(MAX_DECODED_BYTES));
                cache.decoded_bytes = MAX_DECODED_BYTES;
            } else {
                for token in 0..u32::try_from(MAX_STRING_TOKENS)
                    .map_err(|_| ReadError::limit("test cache"))?
                {
                    cache.values.insert((0, token), String::new());
                }
            }
            let retained = (cache.values.len(), cache.decoded_bytes);
            let mut memory = Token;
            let mut reader = TargetReader::new(&mut memory, 0x10000, 0x20000, READ_LIMITS)?;
            for _ in 0..2 {
                assert_eq!(cache.resolve(&mut reader, 0x10000, 1)?, "Item");
                assert_eq!((cache.values.len(), cache.decoded_bytes), retained);
            }
        }
        Ok(())
    }
}
