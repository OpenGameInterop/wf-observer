//! The game's 16-byte inline/external UTF-8 strings, used by names and chat.

use memory_reader::ProcessMemory;
use provider_sdk::memory::{ObjectOffset, ReadError, TargetReader, read_stable};

pub(crate) fn read_string(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    object: u64,
    offset: ObjectOffset,
    maximum: usize,
) -> Result<String, ReadError> {
    let address = reader.object_address(object, offset, 16)?;
    read_stable(
        || {
            let header = reader.read_array::<16>(address)?;
            let bytes = match decode_header(&header, maximum)? {
                Storage::Inline(length) => header[..length].to_vec(),
                Storage::External(pointer, length) => {
                    let mut bytes = vec![0; length];
                    if length != 0 {
                        reader.read_at(pointer, &mut bytes)?;
                    }
                    bytes
                }
            };
            if reader.read_array::<16>(address)? != header {
                return Err(ReadError::changed("native string header"));
            }
            if bytes.contains(&0) {
                return Err(ReadError::invalid("native string NUL"));
            }
            String::from_utf8(bytes).map_err(|_| ReadError::invalid("native string UTF-8"))
        },
        || ReadError::changed("native string contents"),
    )
}

#[derive(Debug, ..Copy, ..Eq)]
enum Storage {
    Inline(usize),
    External(u64, usize),
}

fn decode_header(header: &[u8; 16], maximum: usize) -> Result<Storage, ReadError> {
    let (storage, length) = match header[15] {
        unused @ 0..=15 => {
            let length = usize::from(15 - unused);
            (Storage::Inline(length), length)
        }
        0xff => {
            let pointer = u64::from_le_bytes(header.as_chunks::<8>().0[0]);
            let length = (u32::from_le_bytes(header.as_chunks::<4>().0[2]) & 0x0fff_ffff) as usize;
            if length != 0 && pointer == 0 {
                return Err(ReadError::invalid("native string pointer"));
            }
            (Storage::External(pointer, length), length)
        }
        _ => return Err(ReadError::invalid("native string representation")),
    };
    if length > maximum {
        return Err(ReadError::limit("native string length"));
    }
    Ok(storage)
}

pub(crate) fn player_name(value: &str) -> &str {
    value.trim_end_matches(|c| matches!(u32::from(c), 0xe000..=0xf8ff))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_lengths_are_bounded_before_reading_storage() -> Result<(), ReadError> {
        let mut header = [0; 16];
        header[15] = 12;
        assert_eq!(decode_header(&header, 3)?, Storage::Inline(3));
        assert!(decode_header(&header, 2).is_err());
        header[15] = 16;
        assert!(decode_header(&header, 128).is_err());
        header[15] = 0xff;
        header[..8].copy_from_slice(&0x10000_u64.to_le_bytes());
        header[8..12].copy_from_slice(&129_u32.to_le_bytes());
        assert!(decode_header(&header, 128).is_err());
        assert_eq!(player_name("Player\u{e000}"), "Player");
        Ok(())
    }
}
