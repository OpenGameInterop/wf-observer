//! UI-owned signals fed by ordinary SDK streams.
use dioxus::prelude::*;
use std::{future::Future, sync::Arc};
use wf_observer_sdk::{ObserverClient, ObserverError, StreamExt as _, WarframeSession};

#[derive(Clone)]
pub struct Connection(pub Arc<ObserverClient>);
impl PartialEq for Connection {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Clone)]
pub struct Session(pub WarframeSession);
impl PartialEq for Session {
    fn eq(&self, other: &Self) -> bool {
        self.0.info() == other.0.info()
    }
}

pub struct Live<T: Clone + 'static> {
    pub value: Signal<Option<T>>,
    pub error: Signal<Option<String>>,
}

/// Dioxus owns the signals and component lifetime. The SDK stream owns demand;
/// cancelling this component's future drops its watch even during setup/receive.
pub fn use_watch<T, F, Fut>(open: F, on_item: Option<EventHandler<T>>) -> Live<T>
where
    T: Clone + 'static,
    F: Fn() -> Fut + Clone + 'static,
    Fut: Future<
            Output = Result<n0_future::boxed::BoxStream<Result<T, ObserverError>>, ObserverError>,
        > + 'static,
{
    let mut value = use_signal(|| None);
    let mut error = use_signal(|| None);
    use_future(move || {
        let open = open.clone();
        async move {
            let result = async {
                let mut updates = open().await?;
                while let Some(item) = updates.next().await {
                    let item = item?;
                    value.set(Some(item.clone()));
                    if let Some(callback) = on_item {
                        callback.call(item);
                    }
                }
                Ok::<_, ObserverError>(())
            }
            .await;
            value.set(None);
            if let Err(problem) = result {
                error.set(Some(problem.to_string()));
            }
        }
    });
    Live { value, error }
}

pub async fn request<T, E: std::fmt::Display>(
    future: impl Future<Output = Result<T, E>>,
) -> Result<T, String> {
    future.await.map_err(|error| error.to_string())
}

pub fn health(value: &wf_observer_sdk::CapabilityHealth) -> String {
    match value {
        wf_observer_sdk::CapabilityHealth::Idle => "Idle · no demand".into(),
        wf_observer_sdk::CapabilityHealth::Initializing => "Waiting for a sample".into(),
        wf_observer_sdk::CapabilityHealth::Available => "Available".into(),
        wf_observer_sdk::CapabilityHealth::Unavailable { reason } => {
            format!("Unavailable · {reason}")
        }
    }
}
