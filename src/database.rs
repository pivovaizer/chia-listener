use tokio_postgres::{Client, NoTls};
use tracing::info;

// Embeds every file from ./migrations into the binary at compile time.
mod embedded {
    refinery::embed_migrations!("migrations");
}

pub struct Database {
    client: Client,
}

impl Database {
    pub async fn connect() -> Self {
        dotenvy::dotenv().expect("Failed to load .env file");
        let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

        let (mut client, connection) =
            tokio_postgres::connect(&db_url, NoTls).await.expect("DB connection failed");

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("DB connection error: {}", e);
            }
        });

        // Apply pending migrations; already applied ones are skipped.
        let report = embedded::migrations::runner()
            .run_async(&mut client)
            .await
            .expect("DB migrations failed");
        for m in report.applied_migrations() {
            info!("Applied migration {}", m);
        }

        Database { client }
    }

    // TODO: find_gaps() — scan blocks table for missing heights
    // TODO: backfill missing blocks from peers into DB
    // TODO: live sync — write new blocks to DB as they arrive

    pub async fn get_max_height(&self) -> Option<i64> {
        let row = self
            .client
            .query_one("SELECT MAX(height) FROM blocks", &[])
            .await
            .expect("query failed");
        row.get(0)
    }
}
