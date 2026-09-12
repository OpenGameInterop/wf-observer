//! Native discovery and attachment adapter for the lifecycle worker.

use anyhow::Context as _;
use memory_reader::AttachedTarget;

use crate::{
    provider_host::HostedSession,
    providers::{self, Candidate},
    runtime::TargetInfo,
};

// This is a local lifecycle test seam, not the provider contract.
pub(super) trait Backend {
    type Candidate;
    type Attachment;

    fn discover(&mut self) -> anyhow::Result<Vec<Self::Candidate>>;
    fn target(candidate: &Self::Candidate) -> TargetInfo;
    fn attach(&mut self, target: &Self::Candidate) -> anyhow::Result<Self::Attachment>;
    fn is_current(&mut self, attachment: &Self::Attachment) -> bool;
}

pub(super) struct NativeBackend;

impl Backend for NativeBackend {
    type Candidate = Candidate;
    type Attachment = HostedSession<AttachedTarget>;

    fn discover(&mut self) -> anyhow::Result<Vec<Candidate>> {
        providers::discover()
    }

    fn target(candidate: &Candidate) -> TargetInfo {
        let manifest = candidate.provider.manifest();
        TargetInfo {
            process: candidate.target.instance().into(),
            executable: candidate.target.executable().to_owned(),
            provider_id: manifest.id.to_owned(),
            game_id: manifest.game.id.to_owned(),
        }
    }

    fn attach(&mut self, candidate: &Candidate) -> anyhow::Result<Self::Attachment> {
        let memory = memory_reader::attach(&candidate.target).with_context(|| {
            format!(
                "failed to attach to process {}",
                candidate.target.instance().pid()
            )
        })?;
        HostedSession::start(candidate.provider, memory)
    }

    fn is_current(&mut self, attachment: &Self::Attachment) -> bool {
        attachment.memory.target().instance().is_current()
    }
}
