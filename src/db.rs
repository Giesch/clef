use std::time::Duration;

use camino::Utf8Path;
use diesel::r2d2::ConnectionManager;
use diesel::sqlite::SqliteConnection;
use r2d2::Pool;

pub mod models;
pub mod schema;

#[derive(thiserror::Error, Debug, Clone)]
pub enum ConnectionError {
    #[error("no user config dir available")]
    NoConfigDir,
    #[error("database connection error")]
    ConnectionError,
}

pub type SqlitePool = Pool<ConnectionManager<SqliteConnection>>;

const CONNECTION_TIMEOUT: Duration = Duration::from_secs(1);

/// Takes a directory, and returns a sqlite connection pool
pub fn create_pool(dir: &Utf8Path) -> Result<SqlitePool, r2d2::Error> {
    let db_url = dir.join("db.sqlite");
    let manager = ConnectionManager::new(db_url.as_str());

    Pool::builder()
        .connection_timeout(CONNECTION_TIMEOUT)
        .connection_customizer(Box::new(ConnectionOptions {
            enable_wal: true,
            enable_foreign_keys: true,
            busy_timeout: Some(Duration::from_secs(30)),
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

            if self.enable_wal {
                conn.batch_execute(
                    "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;",
                )?;
            }
            if self.enable_foreign_keys {
                conn.batch_execute("PRAGMA foreign_keys = ON;")?;
            }
            if let Some(d) = self.busy_timeout {
                conn.batch_execute(&format!("PRAGMA busy_timeout = {};", d.as_millis()))?;
            }
            Ok(())
        })()
        .map_err(diesel::r2d2::Error::QueryError)
    }
}
