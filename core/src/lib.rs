//! VegamShare core: pairing, discovery and file transfer over the local network.
//!
//! See `docs/PROTOCOL.md` and `docs/SECURITY.md` for the design.

mod discovery;
pub mod error;
pub mod identity;
mod node;
mod pairing;
pub mod pathsafe;
pub mod proto;
pub mod secret;
pub mod store;
mod tls;
pub mod transfer;

pub use discovery::local_addresses;
pub use error::{Error, Result};
pub use identity::Fingerprint;
pub use node::{Config, ContentOpener, DEFAULT_DISCOVERY_PORT, DEFAULT_PORT, DeviceView, Event, MyInfo, NearbyView, Node};
pub use pairing::{MAX_PASSWORD_LEN, MIN_PASSWORD_LEN};
pub use store::{HistoryEntry, TrustedDevice};
pub use transfer::{Direction, Source, TransferStatus, TransferView};
