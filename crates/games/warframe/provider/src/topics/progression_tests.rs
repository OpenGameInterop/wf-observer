//! Layout proofs use observed executable bytes, not bytes assembled from facts.
use super::{progression_windows::WINDOWS, validate_intrinsics_layout, validate_star_chart_layout};
use memory_reader::{AccessError, MemoryModule, ProcessMemory, Target};
use provider_sdk::memory::ReadError;
use std::collections::BTreeMap;

pub(crate) struct Image {
    pub(crate) bytes: BTreeMap<u64, u8>,
}

impl Image {
    pub(crate) fn new(base: u64) -> Self {
        let mut image = Self {
            bytes: BTreeMap::new(),
        };
        image.extend(base, WINDOWS);
        image
    }

    pub(crate) fn extend(&mut self, base: u64, windows: &[(u32, &[u8])]) {
        for &(rva, code) in windows {
            self.bytes
                .extend((base + u64::from(rva)..).zip(code.iter().copied()));
        }
    }
}

impl ProcessMemory for Image {
    fn target(&self) -> &Target {
        unreachable!("layout-only fixture")
    }
    fn verify(&mut self) -> Result<(), AccessError> {
        unreachable!("host verifies target")
    }
    fn modules(&mut self) -> Result<Vec<MemoryModule>, AccessError> {
        unreachable!("known image")
    }
    fn read_into(&mut self, address: u64, output: &mut [u8]) -> Result<(), AccessError> {
        let length = output.len();
        for (at, byte) in (address..).zip(output) {
            *byte = *self
                .bytes
                .get(&at)
                .ok_or(AccessError::InvalidReadRange { address, length })?;
        }
        Ok(())
    }
}

#[test]
fn observed_intrinsics_and_missions_layouts_reject_changed_semantics()
-> Result<(), Box<dyn std::error::Error>> {
    for base in [0x1_4000_0000, 0x2_8000_0000] {
        let mut image = Image::new(base);
        validate_intrinsics_layout(&mut image, base, crate::target::BUILD.image_size)?;
        validate_star_chart_layout(&mut image, base, crate::target::BUILD.image_size)?;
        for rva in [
            0x00af_43e1,
            0x00e5_8120,
            0x00e5_81a1,
            0x0164_c129,
            0x010c_a514,
            0x010c_a54c,
            0x0008_4f74,
            0x0226_9b58,
        ] {
            let mut image = Image::new(base);
            *image
                .bytes
                .get_mut(&(base + rva))
                .ok_or("missing instruction")? ^= 1;
            assert!(
                matches!(
                    validate_intrinsics_layout(&mut image, base, crate::target::BUILD.image_size),
                    Err(ReadError::LayoutMismatch { .. })
                ),
                "{rva:x}"
            );
            validate_star_chart_layout(&mut image, base, crate::target::BUILD.image_size)?;
        }
        for rva in [
            0x018f_1a13,
            0x00af_10fe,
            0x022a_4880,
            0x0100_62e7,
            0x0100_6307,
            0x0100_633d,
            0x00ec_31ba,
        ] {
            let mut image = Image::new(base);
            *image
                .bytes
                .get_mut(&(base + rva))
                .ok_or("missing instruction")? ^= 1;
            assert!(
                matches!(
                    validate_star_chart_layout(&mut image, base, crate::target::BUILD.image_size),
                    Err(ReadError::LayoutMismatch { .. })
                ),
                "{rva:x}"
            );
            validate_intrinsics_layout(&mut image, base, crate::target::BUILD.image_size)?;
        }
    }
    Ok(())
}
