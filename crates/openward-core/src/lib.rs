pub mod identifiers;
pub mod dates;
pub mod errors;
pub mod legal;
pub mod warrant;
pub mod detainee;
pub mod analytics;
pub mod traits;

// Re-export everything for flat access: `use openward_core::DetaineeId;`
pub use identifiers::*;
pub use dates::*;
pub use errors::*;
pub use legal::*;
pub use warrant::*;
pub use detainee::*;
pub use analytics::*;
pub use traits::*;
