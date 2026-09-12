//! Profile data → channel lists → retained messages, using [`facts::CHAT`].
//! Head/tail probes avoid copying unchanged history. The cursor converts coherent
//! readings into bounded event batches; the session commits its position only
//! after host publication succeeds.

mod acquisition;
mod cursor;
mod facts;
mod layout;
mod validation;

pub(crate) use acquisition::{History, read_chat};
pub(crate) use cursor::ChatCursor;
pub(crate) use validation::validate_chat_layout;
