use tokio_postgres::{Client, NoTls};

pub struct Database {
    client: Client,
}

impl Database {
    pub async fn connect() -> Self {
        dotenvy::dotenv().expect("Failed to load .env file");
        let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

        let (client, connection) =
            tokio_postgres::connect(&db_url, NoTls).await.expect("DB connection failed");

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("DB connection error: {}", e);
            }
        });

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
