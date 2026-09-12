use crate::raw::{Client, ClientError, Subscription, Topic, TypedData, decode_snapshot, types};
use warframe_model::CurrencySnapshot;

/// Credits, Endo and tradable/non-tradable Platinum, published as full replacements.
pub struct CurrenciesTopic;

impl Topic for CurrenciesTopic {
    const GAME_ID: &'static str = "warframe";
    const PROVIDER_ID: &'static str = "opengameinterop.warframe";
    const NAME: &'static str = "warframe.currencies";
    const SCHEMA_VERSION: u32 = 1;
}

impl crate::raw::SnapshotTopic for CurrenciesTopic {
    type Snapshot = CurrencySnapshot;
}

/// Checks topic/schema, game identity and the complete currency payload.
///
/// # Errors
///
/// Returns identity or model validation errors.
pub fn decode_currencies(
    envelope: types::DataEnvelope,
) -> Result<TypedData<CurrencySnapshot>, ClientError> {
    if envelope.metadata.source.game_id != "warframe" {
        return Err(ClientError::WrongTopic);
    }
    decode_snapshot::<CurrenciesTopic>(envelope)
}

impl Client {
    /// Queries one session's cached balances. Does not start sampling.
    ///
    /// # Errors
    ///
    /// Returns snapshot request, identity or model validation errors.
    pub async fn currencies_snapshot(
        &self,
        session: &types::SessionRef,
    ) -> Result<TypedData<CurrencySnapshot>, ClientError> {
        decode_currencies(self.snapshot(session, &CurrenciesTopic::topic()).await?)
    }

    /// Listens to currencies, sharing existing SDK feeds.
    /// Decode snapshots from the listener's state with [`decode_currencies`].
    ///
    /// # Errors
    ///
    /// Returns connection, protocol or subscription rejection errors.
    pub async fn subscribe_currencies(
        &self,
        sessions: types::SessionSelector,
    ) -> Result<Subscription, ClientError> {
        self.subscribe(types::Subscribe {
            sessions,
            topics: vec![CurrenciesTopic::topic()],
        })
        .await
    }
}
