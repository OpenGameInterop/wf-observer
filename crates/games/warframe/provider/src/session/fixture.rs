//! Sparse synthetic objects for acquisition tests; no executable code is modeled.

use std::collections::BTreeMap;

use memory_reader::{AccessError, MemoryModule, ProcessMemory, Target};

pub(super) const ACCOUNT: &[u8; 24] = b"0123456789abcdef01234567";
pub(super) const OTHER_ACCOUNT: &[u8; 24] = b"abcdef0123456789abcdef01";

pub(super) struct Memory {
    base: u64,
    bytes: BTreeMap<u64, u8>,
    change: Option<(u64, u64, Vec<u8>)>,
}

impl Memory {
    pub(super) fn new(base: u64) -> Self {
        let mut memory = Self {
            base,
            bytes: BTreeMap::new(),
            change: None,
        };
        memory.login();
        memory.inventory();
        memory
    }

    pub(super) const fn heap(&self) -> u64 {
        self.base + 0x1000_0000
    }

    pub(super) const fn account(&self) -> u64 {
        self.heap() + 0x5_0000
    }

    pub(super) const fn data(&self) -> u64 {
        self.heap() + 0x10_0000
    }

    pub(super) const fn payload(&self) -> u64 {
        self.heap() + 0x20_0000
    }

    pub(super) fn put(&mut self, address: u64, bytes: &[u8]) {
        self.bytes.extend((address..).zip(bytes.iter().copied()));
    }

    pub(super) fn omit(&mut self, address: u64) {
        self.bytes.remove(&address);
    }

    pub(super) fn change_on_read(&mut self, trigger: u64, address: u64, bytes: &[u8]) {
        self.change = Some((trigger, address, bytes.to_vec()));
    }

    fn login(&mut self) {
        let base = self.base;
        let heap = self.heap();
        let manager = heap + 0x1_0000;
        let vector = heap + 0x2_0000;
        let control = heap + 0x3_0000;
        let profile = heap + 0x4_0000;
        let account = self.account();
        let data_control = heap + 0x6_0000;
        // Fixed fixture offsets intentionally do not follow production facts.
        for (address, pointer) in [
            (base + 0x027a_53c0, heap),
            (heap, manager),
            (manager, base + 0x0214_ba70),
            (base + 0x0214_ba70 + 0xb8, base + 0x0020_9260),
            (base + 0x0214_ba70 + 0x148, base + 0x00f7_a940),
            (manager + 0x230, vector),
            (vector, control),
            (control, profile),
            (profile, base + 0x0215_1328),
            (base + 0x0215_1328 + 0x18, base + 0x0024_4ee0),
            (base + 0x0215_1328 + 0x28, base + 0x0056_ee90),
            (base + 0x0215_1328 + 0x390, base + 0x008e_1ff0),
            (profile + 0x1d8, 0),
            (profile + 0x208, data_control),
            (data_control, self.data()),
            (self.data(), base + 0x0237_e5c0),
        ] {
            self.put(address, &pointer.to_le_bytes());
        }
        self.put(manager + 0x168, &4_u32.to_le_bytes());
        self.put(manager + 0x238, &8_u32.to_le_bytes());
        self.put(manager + 0x23c, &8_u32.to_le_bytes());
        self.put(profile + 0x1e1, &[1]);
        self.put(profile + 0x118, &[0; 16]);
        self.put(profile + 0x118, &account.to_le_bytes());
        self.put(profile + 0x120, &24_u32.to_le_bytes());
        self.put(profile + 0x127, &[0xff]);
        self.put(account, ACCOUNT);
    }

    fn inventory(&mut self) {
        self.put(self.data() + 0x11c60, &[0]);
        self.put(self.data() + 0xfdc0, &[0; 24]);
        for offset in [
            0x0, 0x10, 0x20, 0x30, 0x50, 0x60, 0xb0, 0xd0, 0x100, 0x110, 0x178, 0x198, 0x1a8,
            0x1b8, 0x208, 0x228, 0x1d8, 0x1e8, 0x1c8, 0x120, 0xe0, 0x258, 0x268, 0x278, 0x288,
            0x298, 0x2f8, 0x308, 0x2a8, 0x2b8, 0x2c8, 0x2d8, 0x358, 0x368, 0x378, 0x388, 0x398,
            0x3a8, 0x3b8, 0xae0,
        ] {
            self.put(self.data() + 0xd5d0 + offset, &[0; 16]);
        }
        let first_type = self.heap() + 0x30_0000;
        let second_type = self.heap() + 0x31_0000;
        let header = self.data() + 0xd5d0 + 0xd0;
        self.put(header, &self.payload().to_le_bytes());
        self.put(header + 8, &48_u32.to_le_bytes());
        self.put(header + 12, &48_u32.to_le_bytes());
        for (index, (item, quantity)) in [(first_type, 3_u32), (second_type, 5), (first_type, 7)]
            .into_iter()
            .enumerate()
        {
            let record = self.payload() + index as u64 * 16;
            let address_word = ((record + 12) >> 3).to_le_bytes();
            let address_word = u32::from_le_bytes(address_word.as_chunks::<4>().0[0]);
            let stored = (quantity ^ address_word ^ 0xc551_98a3).rotate_right(19);
            self.put(record, &item.to_le_bytes());
            self.put(record + 8, &(stored ^ 0xad84_b2ea).to_le_bytes());
            self.put(record + 12, &stored.to_le_bytes());
        }
        let pool = self.heap() + 0x32_0000;
        let prefix = self.heap() + 0x33_0000;
        self.put(self.base + 0x028a_39a0, &pool.to_le_bytes());
        self.put(prefix, &1_u32.to_le_bytes());
        for (address, token) in [(first_type, 2_u32), (second_type, 3)] {
            self.put(address, &[0; 48]);
            self.put(address, &(self.base + 0x0203_fba8).to_le_bytes());
            self.put(address + 16, &prefix.to_le_bytes());
            self.put(address + 44, &token.to_le_bytes());
        }
        for (token, text) in [
            (1_u64, "/Lotus/Types/Items/MiscItems/"),
            (2, "AlloyPlate"),
            (3, "OrokinCell"),
        ] {
            let address = self.heap() + 0x34_0000 + token * 0x1000;
            self.put(pool + token * 16, &address.to_le_bytes());
            self.put(address, &[0; 64]);
            self.put(address, text.as_bytes());
        }
    }
}

impl ProcessMemory for Memory {
    fn target(&self) -> &Target {
        unreachable!("acquisition tests assume an identified executable")
    }

    fn verify(&mut self) -> Result<(), AccessError> {
        unreachable!("the host owns process verification")
    }

    fn modules(&mut self) -> Result<Vec<MemoryModule>, AccessError> {
        unreachable!("acquisition tests assume an identified executable")
    }

    fn read_into(&mut self, address: u64, output: &mut [u8]) -> Result<(), AccessError> {
        if self
            .change
            .as_ref()
            .is_some_and(|change| change.0 == address)
            && let Some((_, at, bytes)) = self.change.take()
        {
            self.put(at, &bytes);
        }
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
