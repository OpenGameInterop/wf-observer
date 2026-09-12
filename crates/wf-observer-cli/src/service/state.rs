//! Shared state ownership, bounded reads, and demand wakeups.

use super::{catalog, inner::Inner};
use parking_lot::{Condvar, Mutex};
use protocol::v1 as wire;
use provider_sdk::ProviderManifest;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::time::Instant;

pub(crate) const POLL_GRACE: Duration = Duration::from_secs(5);

pub(super) struct Shared {
    pub inner: Mutex<Inner>,
    pub changed: Condvar,
}

#[derive(Clone)]
pub(crate) struct ServiceState {
    pub(super) shared: Arc<Shared>,
}

/// Deliberately has no publication, attachment, or provider methods.
#[derive(Clone)]
pub(crate) struct ServiceView {
    pub(super) shared: Arc<Shared>,
}

impl ServiceState {
    #[cfg(test)]
    pub(crate) fn set_message_limit(&self, bytes: u32) {
        self.shared.inner.lock().message_bytes = bytes;
    }

    #[cfg(test)]
    pub(crate) fn subscription_count(&self) -> usize {
        self.shared.inner.lock().subscribers.len()
    }

    pub(crate) fn new(manifests: &[&ProviderManifest]) -> anyhow::Result<Self> {
        let mut run = [0_u8; 16];
        getrandom::fill(&mut run)?;
        Ok(Self {
            shared: Arc::new(Shared {
                inner: Mutex::new(Inner {
                    catalog: catalog::catalog(manifests)?,
                    message_bytes: u32::try_from(irpc::rpc::MAX_MESSAGE_SIZE).unwrap_or(u32::MAX),
                    run_id: format!("{:032x}", u128::from_le_bytes(run)),
                    sequence: 0,
                    runtime: crate::runtime::HostStatus::default(),
                    sessions: BTreeMap::new(),
                    subscribers: BTreeMap::new(),
                    next_subscription: 0,
                    revision: 0,
                    stopped: false,
                }),
                changed: Condvar::new(),
            }),
        })
    }

    pub(crate) fn view(&self) -> ServiceView {
        ServiceView {
            shared: self.shared.clone(),
        }
    }

    pub(crate) fn update_lifecycle(
        &self,
        status: crate::runtime::HostStatus,
    ) -> Result<(), wire::RequestError> {
        let result = self.shared.inner.lock().sync_runtime(status);
        self.shared.changed.notify_all();
        result
    }

    pub(crate) fn revision(&self) -> u64 {
        self.shared.inner.lock().revision
    }

    pub(crate) fn wait(&self, revision: u64, duration: Duration) {
        let mut inner = self.shared.inner.lock();
        if inner.revision == revision && !inner.stopped {
            self.shared
                .changed
                .wait_for(&mut inner, duration.min(Duration::from_millis(100)));
        }
    }

    pub(crate) fn expire(&self, now: Instant) {
        self.shared.inner.lock().expire(now);
        self.shared.changed.notify_all();
    }

    pub(crate) fn shutdown(&self) {
        self.shared
            .inner
            .lock()
            .close(&wire::SubscriptionEnd::ServiceStopped);
        self.shared.changed.notify_all();
    }
}

impl ServiceView {
    pub(crate) fn message_bytes(&self) -> u32 {
        self.shared.inner.lock().message_bytes
    }

    pub(crate) fn catalog(&self) -> Result<wire::Catalog, wire::RequestError> {
        let inner = self.shared.inner.lock();
        catalog::message_fits(
            &Ok::<_, wire::RequestError>(&inner.catalog),
            inner.message_bytes,
        )?;
        Ok(inner.catalog.clone())
    }

    pub(crate) fn status(&self) -> Result<wire::ServiceStatus, wire::RequestError> {
        let mut inner = self.shared.inner.lock();
        inner.expire(Instant::now());
        let targets = inner
            .runtime
            .targets
            .iter()
            .map(|target| {
                let activity = match &target.activity {
                    crate::runtime::Activity::Attaching => wire::TargetActivity::Attaching,
                    crate::runtime::Activity::Retrying { .. } => wire::TargetActivity::Retrying {
                        message: "attachment failed; see local service logs".into(),
                    },
                    crate::runtime::Activity::Observing { session_id } => {
                        let session = inner.sessions.get(session_id);
                        wire::TargetActivity::Observing {
                            session_id: session_id.clone(),
                            game_build: session.and_then(|s| s.info.game_build.clone()),
                            topics: session
                                .map(|s| {
                                    s.topics
                                        .values()
                                        .map(|t| wire::TopicStatus {
                                            topic: t.state.source.topic.clone(),
                                            generation: t.state.generation,
                                            health: t.state.health.clone(),
                                        })
                                        .collect()
                                })
                                .unwrap_or_default(),
                        }
                    }
                };
                wire::TargetStatus {
                    provider_id: target.target.provider_id.clone(),
                    game_id: target.target.game_id.clone(),
                    target: wire::TargetProcess {
                        pid: target.target.process.pid,
                        executable: target.target.executable.clone(),
                    },
                    activity,
                }
            })
            .collect();
        let status = wire::ServiceStatus {
            cursor: inner.cursor(),
            application_version: env!("CARGO_PKG_VERSION").into(),
            discovery: if inner.runtime.discovery_error.is_some() {
                wire::DiscoveryHealth::Retrying {
                    message: "process enumeration failed".into(),
                }
            } else {
                wire::DiscoveryHealth::Searching
            },
            targets,
        };
        catalog::message_fits(&Ok::<_, wire::RequestError>(&status), inner.message_bytes)?;
        Ok(status)
    }

    pub(crate) fn snapshot(
        &self,
        request: &wire::GetSnapshot,
    ) -> Result<wire::DataEnvelope, wire::RequestError> {
        let mut inner = self.shared.inner.lock();
        inner.expire(Instant::now());
        let session = inner.session(&request.session)?;
        let cap = inner.capability(&request.topic)?;
        if request.topic.provider_id != session.info.provider_id {
            return Err(catalog::invalid("topic does not belong to session"));
        }
        if !cap.snapshots {
            return Err(wire::RequestError::SnapshotsUnsupported {
                topic: request.topic.clone(),
            });
        }
        let topic = session
            .topics
            .get(&request.topic)
            .ok_or_else(|| catalog::invalid("session capability is missing"))?;
        match &topic.state.health {
            wire::CapabilityHealth::Idle => Err(wire::RequestError::Idle),
            wire::CapabilityHealth::Initializing => Err(wire::RequestError::NotSampled),
            wire::CapabilityHealth::Unavailable { reason } => {
                Err(wire::RequestError::Unavailable {
                    reason: reason.clone(),
                })
            }
            wire::CapabilityHealth::Available => topic
                .state
                .snapshot
                .clone()
                .ok_or(wire::RequestError::NotSampled),
        }
    }
}
