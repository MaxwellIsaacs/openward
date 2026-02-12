//! SQLite storage layer — schema, migrations, connection pool.

pub mod migrations;
pub mod pool;

pub use migrations::apply_schema;
pub use pool::create_pool;
