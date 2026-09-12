use crate::raw::{Client, ClientError, Subscription, Topic, TypedData, decode_snapshot, types};
use warframe_model::InventorySnapshot;

/// Full validated inventory replacements. Subscriptions also deliver generic
/// lifecycle state managed by the subscription.
pub struct InventoryTopic;

impl Topic for InventoryTopic {
    const GAME_ID: &'static str = "warframe";
    const PROVIDER_ID: &'static str = "opengameinterop.warframe";
    const NAME: &'static str = "warframe.inventory";
    const SCHEMA_VERSION: u32 = 1;
}

impl crate::raw::SnapshotTopic for InventoryTopic {
    type Snapshot = InventorySnapshot;
}

/// Validates topic/schema and game identity before decoding a complete inventory.
///
/// # Errors
///
/// Returns identity or model validation errors.
pub fn decode_inventory(
    envelope: types::DataEnvelope,
) -> Result<TypedData<InventorySnapshot>, ClientError> {
    if envelope.metadata.source.game_id != "warframe" {
        return Err(ClientError::WrongTopic);
    }
    decode_snapshot::<InventoryTopic>(envelope)
}

impl Client {
    /// Queries a cached inventory for one explicit session. Does not start sampling.
    ///
    /// # Errors
    ///
    /// Returns snapshot request, identity, or model validation errors.
    pub async fn inventory_snapshot(
        &self,
        session: &types::SessionRef,
    ) -> Result<TypedData<InventorySnapshot>, ClientError> {
        decode_inventory(self.snapshot(session, &InventoryTopic::topic()).await?)
    }

    /// Listens to inventory for the chosen sessions, sharing existing SDK feeds.
    /// Decode snapshots from the listener's state with [`decode_inventory`].
    /// Close/drop the listener to release its demand.
    ///
    /// # Errors
    ///
    /// Returns connection, protocol, or subscription rejection errors.
    pub async fn subscribe_inventory(
        &self,
        sessions: types::SessionSelector,
    ) -> Result<Subscription, ClientError> {
        self.subscribe(types::Subscribe {
            sessions,
            topics: vec![InventoryTopic::topic()],
        })
        .await
    }
}
