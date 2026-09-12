//! Session-bound handles shared by Rust and generated language bindings.
use crate::api::{ObserverError, SessionInfo, runtime};

pub struct Warframe {
    client: crate::client::Client,
}
impl Warframe {
    pub(crate) fn new(client: crate::client::Client) -> Self {
        Self { client }
    }
}
#[boltffi::export]
impl Warframe {
    /// Lists sessions without creating game-data demand.
    /// # Errors
    /// Reports runtime and status request errors.
    pub async fn sessions(&self) -> Result<Vec<SessionInfo>, ObserverError> {
        let client = self.client.clone();
        runtime::execute(async move {
            let status = client.status().await?;
            Ok::<_, crate::raw::ClientError>(
                status
                    .targets
                    .into_iter()
                    .filter_map(|target| {
                        if target.game_id != "warframe"
                            || target.provider_id != "opengameinterop.warframe"
                        {
                            return None;
                        }
                        let crate::raw::types::TargetActivity::Observing {
                            session_id,
                            game_build,
                            ..
                        } = target.activity
                        else {
                            return None;
                        };
                        Some(SessionInfo {
                            session: crate::raw::types::SessionRef {
                                run_id: status.cursor.run_id.clone(),
                                session_id,
                            },
                            provider_id: target.provider_id,
                            game_id: target.game_id,
                            target: target.target,
                            game_build,
                        })
                    })
                    .collect(),
            )
        })
        .await?
        .map_err(Into::into)
    }
    /// Binds a discovered session without starting acquisition.
    /// # Errors
    /// Rejects metadata for a different game or provider.
    pub fn session(&self, info: SessionInfo) -> Result<WarframeSession, ObserverError> {
        if info.game_id != "warframe" || info.provider_id != "opengameinterop.warframe" {
            return Err(ObserverError::PayloadDecode {
                message: "session belongs to another game or provider".into(),
            });
        }
        Ok(WarframeSession {
            client: self.client.clone(),
            info,
        })
    }
    /// Selects exactly one session. Multiple sessions require an explicit choice.
    /// # Errors
    /// Reports no session, ambiguity, or status request errors.
    pub async fn single_session(&self) -> Result<WarframeSession, ObserverError> {
        let mut sessions = self.sessions().await?;
        match sessions.len() {
            0 => Err(ObserverError::NoSession),
            1 => self.session(sessions.pop().ok_or(ObserverError::NoSession)?),
            _ => Err(ObserverError::AmbiguousSession),
        }
    }
}

#[derive(Clone)]
pub struct WarframeSession {
    client: crate::client::Client,
    info: SessionInfo,
}
#[boltffi::export]
impl WarframeSession {
    /// Captured session identity and process metadata.
    #[must_use]
    pub fn info(&self) -> SessionInfo {
        self.info.clone()
    }
    /// Creates an inactive capability handle; retaining it does not acquire data.
    #[must_use]
    pub fn currencies(&self) -> crate::api::CurrenciesCapability {
        crate::api::CurrenciesCapability::new(crate::raw::Capability::<
            crate::warframe::CurrenciesTopic,
        >::new(
            self.client.clone(), self.info.session.clone()
        ))
    }
    /// Creates an inactive capability handle; retaining it does not acquire data.
    #[must_use]
    pub fn inventory(&self) -> crate::api::InventoryCapability {
        crate::api::InventoryCapability::new(crate::raw::Capability::<
            crate::warframe::InventoryTopic,
        >::new(
            self.client.clone(), self.info.session.clone()
        ))
    }
}
