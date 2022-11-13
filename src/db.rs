use std::time::Duration;

use camino::Utf8Path;
use diesel::r2d2::ConnectionManager;
use diesel::sqlite::SqliteConnection;
use r2d2::{Pool, PooledConnection};

pub mod models;
pub mod queries;
pub mod schema;

#[derive(thiserror::Error, Debug, Clone)]
pub enum ConnectionError {
    #[error("no user config dir available")]
    NoConfigDir,
    #[error("database connection error")]
    ConnectionError,
}

pub type SqlitePool = Pool<ConnectionManager<SqliteConnection>>;
pub type SqlitePoolConn = PooledConnection<ConnectionManager<SqliteConnection>>;

const CONNECTION_TIMEOUT: Duration = Duration::from_secs(1);

pub fn create_pool(db_path: &Utf8Path) -> Result<SqlitePool, r2d2::Error> {
    let manager = ConnectionManager::new(db_path.as_str());

    Pool::builder()
        .connection_timeout(CONNECTION_TIMEOUT)
        .connection_customizer(Box::new(ConnectionOptions {
            enable_wal: true,
            enable_foreign_keys: true,
            busy_timeout: Some(Duration::from_secs(5)),
        }))
        .build(manager)
}

// https://stackoverflow.com/questions/57123453/how-to-use-diesel-with-sqlite-connections-and-avoid-database-is-locked-type-of
#[derive(Debug)]
pub struct ConnectionOptions {
    pub enable_wal: bool,
    pub enable_foreign_keys: bool,
    pub busy_timeout: Option<Duration>,
}

impl diesel::r2d2::CustomizeConnection<SqliteConnection, diesel::r2d2::Error>
    for ConnectionOptions
{
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), diesel::r2d2::Error> {
        (|| {
            use diesel::connection::SimpleConnection;

            // NOTE this needs to be first
            // https://github.com/diesel-rs/diesel/issues/2365
            if let Some(d) = self.busy_timeout {
                conn.batch_execute(&format!("PRAGMA busy_timeout = {};", d.as_millis()))?;
            }

            if self.enable_wal {
                conn.batch_execute("PRAGMA journal_mode = WAL;")?;
                conn.batch_execute("PRAGMA synchronous = NORMAL;")?;
            }

            if self.enable_foreign_keys {
                conn.batch_execute("PRAGMA foreign_keys = ON;")?;
            }
            Ok(())
        })()
        .map_err(diesel::r2d2::Error::QueryError)
    }
}
