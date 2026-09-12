//! Compiled Warframe manifest and provider session creation.

use memory_reader::{ProcessMemory, ProcessMetadata, Target};
use provider_sdk::{GameDescriptor, Provider, ProviderError, ProviderManifest, ProviderSession};

use crate::{
    matching,
    session::{INVENTORY, WarframeSession},
};

static MANIFEST: ProviderManifest = ProviderManifest {
    id: "opengameinterop.warframe",
    name: "Warframe",
    version: env!("CARGO_PKG_VERSION"),
    game: GameDescriptor {
        id: "warframe",
        name: "Warframe",
    },
    capabilities: &[INVENTORY],
};

/// Built-in Warframe identification and validated account data acquisition.
#[derive(Debug, Default, ..Copy)]
pub struct WarframeProvider;

impl Provider for WarframeProvider {
    fn manifest(&self) -> &'static ProviderManifest {
        &MANIFEST
    }

    fn identify_process(&self, process: &ProcessMetadata<'_>) -> Option<&'static str> {
        matching::match_process(process)
    }

    fn start(
        &self,
        target: &Target,
        memory: &mut dyn ProcessMemory,
    ) -> Result<Box<dyn ProviderSession>, ProviderError> {
        if matching::matched_target_executable(target.executable()).is_none()
            || memory.target() != target
        {
            return Err(ProviderError::InvalidTarget);
        }
        Ok(Box::new(WarframeSession::default()))
    }
}
