use memory_reader::ProcessMemory;
use provider_sdk::memory::{ReadError, RecordView, TargetReader, read_stable};
use warframe_model::{CurrencyBalances, CurrencySnapshot};

use crate::{roots::LoginIdentity, target::READ_LIMITS};

use super::facts::{BalanceFacts, CODEC, CREDITS, ENDO, NON_TRADABLE_PLATINUM, TRADABLE_PLATINUM};

pub(crate) fn read_currencies(
    memory: &mut (impl ProcessMemory + ?Sized),
    module_base: u64,
    image_size: u32,
    login: &LoginIdentity,
) -> Result<CurrencySnapshot, ReadError> {
    let mut reader = TargetReader::new(memory, module_base, image_size, READ_LIMITS)?;
    let profile = login.profile_data.get();
    let balances = read_stable(
        || {
            Ok(CurrencyBalances {
                credits: read_balance(&mut reader, profile, CREDITS.balance)?,
                endo: read_balance(&mut reader, profile, ENDO)?,
                tradable_platinum: read_balance(&mut reader, profile, TRADABLE_PLATINUM.balance)?,
                non_tradable_platinum: read_balance(
                    &mut reader,
                    profile,
                    NON_TRADABLE_PLATINUM.balance,
                )?,
            })
        },
        || ReadError::changed("currency balances"),
    )?;
    Ok(CurrencySnapshot {
        account_id: login.account_id.clone(),
        balances,
    })
}

fn read_balance(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    profile: u64,
    facts: BalanceFacts,
) -> Result<i32, ReadError> {
    let address = reader.object_address(profile, facts.field, 8)?;
    let bytes = reader.read_array::<8>(address)?;
    let record = RecordView::new(&bytes, "currency balance");
    let stored_address = reader.object_address(address, CODEC.stored, 4)?;
    CODEC
        .decode(
            record.u32(CODEC.check)?,
            record.u32(CODEC.stored)?,
            stored_address,
        )
        .map(u32::cast_signed)
        .ok_or_else(|| ReadError::invalid("currency balance integrity"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roots::ObjectIdentity;
    use memory_reader::{AccessError, MemoryModule, Target};
    use provider_sdk::memory::MemoryError;
    use warframe_model::AccountId;

    const PROFILE: u64 = 0x1_4001_0000;

    // Protected balances at the four profile-data offsets.
    struct Balances {
        words: [[u32; 2]; 4],
        reads: usize,
        change: bool,
        unreadable: bool,
    }

    impl Default for Balances {
        fn default() -> Self {
            Self {
                words: [
                    [0x6609_80bf, 0xcb8d_3255], // i32::MAX Credits
                    [0x99f6_af40, 0x3472_1daa], // zero Endo
                    [0x6609_30bf, 0xcb8d_8255], // -7 tradable Platinum
                    [0x99f0_6f40, 0x3474_ddaa], // 50 non-tradable Platinum
                ],
                reads: 0,
                change: false,
                unreadable: false,
            }
        }
    }

    impl ProcessMemory for Balances {
        fn target(&self) -> &Target {
            unreachable!("acquisition does not identify the process")
        }
        fn verify(&mut self) -> Result<(), AccessError> {
            unreachable!("host verifies the process")
        }
        fn modules(&mut self) -> Result<Vec<MemoryModule>, AccessError> {
            unreachable!("no module reads")
        }
        fn read_into(&mut self, address: u64, output: &mut [u8]) -> Result<(), AccessError> {
            let error = || AccessError::InvalidReadRange {
                address,
                length: output.len(),
            };
            let index = [0xd9a8, 0xd998, 0xd9b0, 0xd9b8]
                .iter()
                .position(|offset| address == PROFILE + offset)
                .ok_or_else(error)?;
            if output.len() != 8 || (self.unreadable && self.reads == 3) {
                return Err(error());
            }
            if self.change && self.reads == 4 {
                self.words[0] = [0x6609_a0bf, 0xcb8d_1255]; // i32::MAX - 1
            }
            output[..4].copy_from_slice(&self.words[index][0].to_le_bytes());
            output[4..].copy_from_slice(&self.words[index][1].to_le_bytes());
            self.reads += 1;
            Ok(())
        }
    }

    #[test]
    fn four_balances_must_be_readable_intact_and_unchanged()
    -> Result<(), Box<dyn std::error::Error>> {
        let object = ObjectIdentity::new(PROFILE).ok_or("invalid address")?;
        let login = LoginIdentity {
            account_id: AccountId::new("0123456789abcdef01234567")?,
            manager_control: object,
            manager: object,
            profile_control: object,
            profile: object,
            profile_data_control: object,
            profile_data: object,
        };
        let sample = |memory: &mut Balances| read_currencies(memory, 0x1_4000_0000, 0x1000, &login);
        let mut memory = Balances::default();
        let snapshot = sample(&mut memory)?;
        assert_eq!(snapshot.account_id, login.account_id);
        assert_eq!(
            snapshot.balances,
            CurrencyBalances {
                credits: i32::MAX,
                endo: 0,
                tradable_platinum: -7,
                non_tradable_platinum: 50,
            }
        );
        assert_eq!(memory.reads, 8);

        let mut corrupt = Balances::default();
        corrupt.words[2][0] ^= 1;
        assert!(matches!(
            sample(&mut corrupt),
            Err(ReadError::InvalidData { .. })
        ));
        let mut changing = Balances {
            change: true,
            ..Balances::default()
        };
        assert!(matches!(
            sample(&mut changing),
            Err(ReadError::Unstable { .. })
        ));
        assert_eq!(changing.reads, 8);
        let mut unreadable = Balances {
            unreadable: true,
            ..Balances::default()
        };
        assert!(matches!(
            sample(&mut unreadable),
            Err(ReadError::Memory(MemoryError::Read(_)))
        ));
        Ok(())
    }
}
