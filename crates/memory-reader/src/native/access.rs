//! Native backend initialization, read primitives, and explicit identity checks.

use memflow::prelude::v1::{OsArgs, Process};
use memflow_native::NativeOs;

use crate::{AccessError, DiscoveryError, MemoryModule, ProcessInstance};

pub(super) fn native_os() -> Result<NativeOs, DiscoveryError> {
    NativeOs::new(&OsArgs::default()).map_err(DiscoveryError::Initialize)
}

pub(super) fn process_modules(
    instance: ProcessInstance,
    process: &mut impl Process,
) -> Result<Vec<MemoryModule>, AccessError> {
    with_current_process(instance, || {
        process
            .module_list()
            .map(|modules| {
                modules
                    .into_iter()
                    .map(|module| MemoryModule {
                        name: module.name.as_ref().to_owned(),
                        path: module.path.as_ref().to_owned(),
                        base: module.base.to_umem(),
                        size: module.size,
                    })
                    .collect()
            })
            .map_err(|source| AccessError::Modules {
                pid: instance.pid(),
                source,
            })
    })
}

pub(super) fn with_current_process<T>(
    instance: ProcessInstance,
    operation: impl FnOnce() -> Result<T, AccessError>,
) -> Result<T, AccessError> {
    ensure_current(instance)?;
    let result = operation();
    ensure_current(instance)?;
    result
}

pub(super) fn ensure_current(instance: ProcessInstance) -> Result<(), AccessError> {
    if instance.is_current() {
        Ok(())
    } else {
        Err(AccessError::TargetChanged(instance.pid()))
    }
}
