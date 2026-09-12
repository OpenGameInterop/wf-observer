use super::{MemoryError, ObjectOffset, ReadError, ReadLimits, Rva, TargetReader};
use memory_reader::{AccessError, MemoryModule, ProcessInstance, ProcessMemory, Target};

type TestResult = Result<(), Box<dyn std::error::Error>>;
const BASE: u64 = 0x1_0000_0000_0000;
const LIMITS: ReadLimits = ReadLimits {
    min_address: BASE,
    max_address: BASE + 31,
    bytes: 1,
    reads: 1,
};

struct Probe {
    target: Target,
    calls: usize,
    fail: bool,
}

impl Probe {
    fn new(fail: bool) -> Result<Self, AccessError> {
        let pid = std::process::id();
        let instance = ProcessInstance::for_pid(pid).ok_or(AccessError::TargetNotFound(pid))?;
        Ok(Self {
            target: Target::new(instance, "reader-test".into()),
            calls: 0,
            fail,
        })
    }
}

impl ProcessMemory for Probe {
    fn target(&self) -> &Target {
        &self.target
    }
    fn verify(&mut self) -> Result<(), AccessError> {
        Ok(())
    }
    fn modules(&mut self) -> Result<Vec<MemoryModule>, AccessError> {
        Ok(Vec::new())
    }
    fn read_into(&mut self, address: u64, buffer: &mut [u8]) -> Result<(), AccessError> {
        self.calls += 1;
        if self.fail {
            return Err(AccessError::InvalidReadRange {
                address,
                length: buffer.len(),
            });
        }
        buffer.fill(0);
        Ok(())
    }
}

#[test]
fn module_and_object_bounds_cannot_bypass_the_target_range() -> TestResult {
    let mut memory = Probe::new(false)?;
    for (base, size) in [(BASE, 0), (BASE - 1, 2), (BASE + 16, 17)] {
        assert!(TargetReader::new(&mut memory, base, size, LIMITS).is_err());
    }
    let wide = ReadLimits {
        max_address: u64::MAX,
        ..LIMITS
    };
    assert!(matches!(
        TargetReader::new(&mut memory, u64::MAX - 1, 4, wide),
        Err(ReadError::Memory(MemoryError::AddressOverflow))
    ));
    assert_eq!(memory.calls, 0);
    let mut reader = TargetReader::new(&mut memory, BASE, 16, LIMITS)?;
    let offset = ObjectOffset::new(1);
    assert!(reader.read_at(BASE - 1, &mut [0]).is_err());
    assert!(matches!(
        reader.read_at(BASE + 31, &mut [0; 2]),
        Err(ReadError::Memory(MemoryError::InvalidTargetRange(..)))
    ));
    assert!(reader.read_module(Rva::new(16), &mut [0]).is_err());
    assert!(reader.object_address(BASE + 31, offset, 1).is_err());
    assert!(matches!(
        reader.object_address(u64::MAX, offset, 1),
        Err(ReadError::Memory(MemoryError::AddressOverflow))
    ));
    reader.read_at(BASE + 31, &mut [0])?;
    assert_eq!(memory.calls, 1);
    Ok(())
}

#[test]
fn all_read_paths_share_budgets_including_backend_failures() -> TestResult {
    for (bytes, reads) in [(12, 99), (99, 3)] {
        for fail in [false, true] {
            let mut memory = Probe::new(fail)?;
            let limits = ReadLimits {
                bytes,
                reads,
                ..LIMITS
            };
            let mut reader = TargetReader::new(&mut memory, BASE, 16, limits)?;
            reader.read_at(BASE, &mut [])?;
            reader.read_module_array::<0>(Rva::new(0))?;
            for result in [
                reader.read_u32(BASE),
                reader
                    .read_module_array::<4>(Rva::new(0))
                    .map(u32::from_le_bytes),
                reader.read_object_u32(BASE, ObjectOffset::new(0)),
            ] {
                if fail {
                    assert!(matches!(
                        result,
                        Err(ReadError::Memory(MemoryError::Read(_)))
                    ));
                } else {
                    result?;
                }
            }
            assert!(matches!(
                reader.read_u8(BASE),
                Err(ReadError::LimitExceeded { .. })
            ));
            assert_eq!(memory.calls, 3);
        }
    }
    Ok(())
}
