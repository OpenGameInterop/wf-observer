use provider_sdk::{PollContext, PollResult, ProviderError, ProviderSession};

pub(crate) struct WarframeSession;

impl ProviderSession for WarframeSession {
    fn poll(&mut self, _context: &mut PollContext<'_>) -> Result<PollResult, ProviderError> {
        Ok(PollResult::Idle)
    }
}
