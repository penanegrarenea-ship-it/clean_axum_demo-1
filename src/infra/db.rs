//! SeaORM database connection setup.

use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbErr};

use crate::common::config::Config;

/// Opens a SeaORM connection pool using the pool sizing from [`Config`].
pub async fn connect(config: &Config) -> Result<DatabaseConnection, DbErr> {
    let mut opts = ConnectOptions::new(config.database_url.clone());
    opts.max_connections(config.database_max_connections)
        .min_connections(config.database_min_connections)
        .sqlx_logging(true)
        .sqlx_logging_level(tracing::log::LevelFilter::Debug);
    Database::connect(opts).await
}

#[cfg(test)]
pub mod test_support {
    use std::sync::Once;

    use sea_orm::DatabaseConnection;

    use crate::common::config::Config;

    static INIT: Once = Once::new();

    /// Connects to the database configured in `.env.test` (seeded via `db-seed/`).
    pub async fn test_db() -> DatabaseConnection {
        INIT.call_once(|| {
            dotenvy::from_filename(".env.test").expect("Failed to load .env.test");
        });
        let config = Config::from_env().expect("config from .env.test");
        super::connect(&config).await.expect("connect to test DB")
    }
}
