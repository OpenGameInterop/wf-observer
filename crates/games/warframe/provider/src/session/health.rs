use crate::{item_type::ItemTypeError, roots::ResolveError, topics::InventoryError};
use provider_sdk::UnavailableReason;

impl From<&ResolveError> for UnavailableReason {
    fn from(error: &ResolveError) -> Self {
        match error {
            ResolveError::Read(error) => error.into(),
            ResolveError::Unsupported(_) => Self::UnsupportedBuild,
            _ => Self::ValidationFailed {
                message: "account ownership could not be validated".into(),
            },
        }
    }
}

impl From<&ItemTypeError> for UnavailableReason {
    fn from(error: &ItemTypeError) -> Self {
        match error {
            ItemTypeError::Read(error) => error.into(),
            ItemTypeError::Invalid(_) => Self::ValidationFailed {
                message: "item type data failed validation".into(),
            },
        }
    }
}

impl From<&InventoryError> for UnavailableReason {
    fn from(error: &InventoryError) -> Self {
        match error {
            InventoryError::Rebuilding => Self::TargetNotReady,
            InventoryError::Read(error) | InventoryError::ItemType(ItemTypeError::Read(error)) => {
                error.into()
            }
            InventoryError::UnsupportedLayout(..) => Self::UnsupportedBuild,
            _ => Self::ValidationFailed {
                message: "inventory records failed validation".into(),
            },
        }
    }
}
