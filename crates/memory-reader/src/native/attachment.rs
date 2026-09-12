//! Retained process attachment and executable-mapping access proof.

#[cfg(not(target_os = "linux"))]
use memflow::prelude::v1::{Address, MemoryView as _};
use memflow::prelude::v1::{ErrorKind, Os, Process};
use memflow_native::NativeProcess;
#[cfg(target_os = "linux")]
use std::{fs::File, os::unix::fs::FileExt as _};

use crate::{AccessError, MemoryModule, ProcessMemory, Target};

use super::access::{ensure_current, native_os, process_modules, with_current_process};

/// A retained native attachment for one discovered process instance.
///
/// Call [`Self::verify`] around each complete acquisition before using its data;
/// individual reads do not recheck identity. Linux reads use an open read-only
/// `/proc/PID/mem` file, retaining the selected address space.
///
/// The memflow-native Windows backend requests write-capable handle permissions;
/// this attachment only performs reads.
pub struct AttachedTarget {
    target: Target,
    probe_address: u64,
    process: NativeProcess,
    #[cfg(target_os = "linux")]
    memory: File,
}

impl AttachedTarget {
    /// Returns the discovered target metadata.
    #[must_use]
    pub const fn target(&self) -> &Target {
        &self.target
    }

    /// Revalidates both the process instance and its readable executable mapping.
    ///
    /// # Errors
    ///
    /// Returns [`AccessError::TargetChanged`] if the PID was reused or the process
    /// exited, and [`AccessError::Read`] if its executable mapping is no longer readable.
    pub fn verify(&mut self) -> Result<(), AccessError> {
        let mut probe = [0_u8; 1];
        with_current_process(self.target.instance(), || {
            self.read_into(self.probe_address, &mut probe)
        })
    }
}

impl ProcessMemory for AttachedTarget {
    fn target(&self) -> &Target {
        self.target()
    }

    fn verify(&mut self) -> Result<(), AccessError> {
        self.verify()
    }

    fn modules(&mut self) -> Result<Vec<MemoryModule>, AccessError> {
        process_modules(self.target.instance(), &mut self.process)
    }

    fn read_into(&mut self, address: u64, buffer: &mut [u8]) -> Result<(), AccessError> {
        u64::try_from(buffer.len())
            .ok()
            .and_then(|length| address.checked_add(length))
            .ok_or(AccessError::InvalidReadRange {
                address,
                length: buffer.len(),
            })?;
        if buffer.is_empty() {
            return Ok(());
        }

        #[cfg(target_os = "linux")]
        let result = self.memory.read_exact_at(buffer, address).map_err(|_| {
            memflow::error::Error(
                memflow::error::ErrorOrigin::OsLayer,
                ErrorKind::UnableToReadMemory,
            )
        });
        #[cfg(not(target_os = "linux"))]
        let result = self
            .process
            .read_raw_into(Address::from(address), buffer)
            .map_err(Into::into);

        result.map_err(|source| AccessError::Read {
            pid: self.target.instance().pid(),
            source,
        })
    }
}

/// Opens a discovered target and proves that its executable mapping is readable.
///
/// Retains native access to the selected process instance and probes the
/// executable/module name supplied by the caller's matcher.
///
/// # Errors
///
/// Returns an error if the target exited, changed identity, cannot be opened, or cannot be read.
pub fn attach(target: &Target) -> Result<AttachedTarget, AccessError> {
    let expected = target.instance();
    ensure_current(expected)?;

    let mut os = native_os().map_err(AccessError::Discovery)?;
    let process_info = os
        .process_info_by_pid(expected.pid())
        .map_err(|source| match source.1 {
            ErrorKind::ProcessNotFound => AccessError::TargetNotFound(expected.pid()),
            _ => AccessError::Open {
                pid: expected.pid(),
                source,
            },
        })?;

    ensure_current(expected)?;
    let mut process =
        os.into_process_by_info(process_info)
            .map_err(|source| AccessError::Open {
                pid: expected.pid(),
                source,
            })?;
    let instance = target.instance();
    let executable = with_current_process(instance, || {
        process
            .module_by_name_ignore_ascii_case(target.executable())
            .map_err(|source| match source.1 {
                ErrorKind::ModuleNotFound => AccessError::NoExecutableModule(instance.pid()),
                _ => AccessError::Modules {
                    pid: instance.pid(),
                    source,
                },
            })
    })?;
    let probe_address = executable.base.to_umem();

    let mut attachment = AttachedTarget {
        target: target.clone(),
        probe_address,
        process,
        #[cfg(target_os = "linux")]
        memory: File::open(format!("/proc/{}/mem", instance.pid())).map_err(|_| {
            AccessError::Open {
                pid: instance.pid(),
                source: memflow::error::Error(
                    memflow::error::ErrorOrigin::OsLayer,
                    ErrorKind::UnableToReadFile,
                ),
            }
        })?,
    };
    attachment.verify()?;
    Ok(attachment)
}
