//! Ordered chat messages, explicit gaps, and current source health.
use crate::api::{
    CapabilityHealth, ChatChannel, ChatMessage, ChatUpdate, EnvelopeMetadata, ObserverError,
    runtime,
};
use crate::warframe::ChatTopic;
use std::sync::Arc;

#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub struct ChatState {
    /// Opaque reset lifetime; absent while acquiring a coherent baseline.
    pub generation: Option<String>,
    pub health: CapabilityHealth,
}
impl From<crate::raw::EventState> for ChatState {
    fn from(state: crate::raw::EventState) -> Self {
        Self {
            generation: state.generation.map(|n| n.to_string()),
            health: state.health,
        }
    }
}

#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub enum ChatObservation {
    State {
        value: ChatState,
    },
    Message {
        metadata: EnvelopeMetadata,
        account_id: String,
        value: ChatMessage,
    },
    Gap {
        metadata: EnvelopeMetadata,
        account_id: String,
        channel: ChatChannel,
    },
}

impl From<crate::raw::EventObservation<warframe_model::ChatEvent>> for ChatObservation {
    fn from(item: crate::raw::EventObservation<warframe_model::ChatEvent>) -> Self {
        match item {
            crate::raw::EventObservation::State(state) => Self::State {
                value: state.into(),
            },
            crate::raw::EventObservation::Event(event) => {
                let metadata = event.metadata.clone().into();
                let account_id = event.data.account_id.as_str().to_owned();
                match &event.data.update {
                    ChatUpdate::Message { value } => Self::Message {
                        metadata,
                        account_id,
                        value: value.clone(),
                    },
                    ChatUpdate::Gap { channel } => Self::Gap {
                        metadata,
                        account_id,
                        channel: *channel,
                    },
                }
            }
        }
    }
}

pub struct ChatCapability {
    inner: crate::raw::EventCapability<ChatTopic>,
}
impl ChatCapability {
    pub(crate) fn new(inner: crate::raw::EventCapability<ChatTopic>) -> Self {
        Self { inner }
    }
}
#[boltffi::export]
impl ChatCapability {
    /// Opens a typed chat watch. Retained game messages are not replayed.
    /// # Errors
    /// Reports subscription setup errors.
    pub async fn watch(&self) -> Result<ChatWatch, ObserverError> {
        let capability = self.inner.clone();
        runtime::execute(async move { capability.watch().await })
            .await?
            .map(|inner| ChatWatch {
                inner: Arc::new(inner),
            })
            .map_err(Into::into)
    }
}

pub struct ChatWatch {
    inner: Arc<crate::raw::EventWatch<ChatTopic>>,
}
#[boltffi::export]
impl ChatWatch {
    /// Reads current source health without consuming any events.
    /// # Errors
    /// Reports closure or identity errors.
    pub fn current(&self) -> Result<ChatState, ObserverError> {
        self.inner.current().map(Into::into).map_err(Into::into)
    }
    /// Receives source state, a message, or an explicit continuity gap.
    /// Only one receive may be pending. Cancelling a receive leaves the watch active.
    /// # Errors
    /// Reports concurrent receive, upstream, lag, and decode failures.
    pub async fn next(&self) -> Result<Option<ChatObservation>, ObserverError> {
        let watch = self.inner.clone();
        runtime::execute(async move { watch.next().await })
            .await?
            .map(|item| item.map(Into::into))
            .map_err(Into::into)
    }
    /// Releases demand immediately and wakes pending receives.
    pub fn cancel(&self) {
        self.inner.close();
    }
    /// Releases demand and waits for pending receives.
    /// # Errors
    /// Reports runtime failure.
    pub async fn shutdown(&self) -> Result<(), ObserverError> {
        let watch = self.inner.clone();
        runtime::execute(async move { watch.shutdown().await }).await
    }
}
impl Drop for ChatWatch {
    fn drop(&mut self) {
        self.inner.close();
    }
}
