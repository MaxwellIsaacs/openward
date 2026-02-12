//! OpenWard Registry Module — SQLite-backed implementation of the Registry trait.
//!
//! The registry is the sole authority on creating and mutating detainee records.
//! No other module writes to the detainees, commitment_orders, charges,
//! court_dates, property_items, or notes tables.

pub mod audit;
pub mod flags;
pub mod queries;
pub mod sqlite;

pub use flags::FacilityConfig;
pub use sqlite::SqliteRegistry;
