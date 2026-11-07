//! Database tests for `Database::insert_block` against a real PostgreSQL.
//!
//! Needs a separate test database (the test wipes its tables!):
//!   docker exec rustlistener createdb -U listener chia_test
//!   TEST_DATABASE_URL="host=127.0.0.1 port=5433 user=listener password=... dbname=chia_test" \
//!     cargo test --test db_insert -- --ignored

use chia_generator_parser::BlockParser;
use chia_listener::database::Database;
use chia_listener::types::BlockReceivedEvent;
use chia_protocol::FullBlock;
use chia_traits::Streamable;
use tokio::sync::{Mutex, MutexGuard};
use tokio_postgres::{Client, NoTls};

const TX_BLOCK: u32 = 9540781;

/// Tests share one database (migrations, TRUNCATE), so they must not run in parallel.
static DB_LOCK: Mutex<()> = Mutex::const_new(());

fn load_event(height: u32) -> BlockReceivedEvent {
    let path = format!("tests/fixtures/block_{height}.bin");
    let bytes = std::fs::read(&path).unwrap_or_else(|_| {
        panic!("missing {path}; create it with: cargo run --example dump_block -- {height}")
    });
    let block = FullBlock::from_bytes(&bytes).unwrap();
    let parsed = BlockParser::new().parse_full_block(&block).unwrap();
    BlockReceivedEvent::from_parsed(&parsed, "test".into())
}

/// Connects to the test DB, refusing anything that does not look like a test database.
/// The returned guard keeps other DB tests waiting until this test finishes.
async fn test_db() -> (MutexGuard<'static, ()>, Database, Client) {
    let guard = DB_LOCK.lock().await;
    let url = std::env::var("TEST_DATABASE_URL").expect("set TEST_DATABASE_URL (see file header)");
    assert!(
        url.contains("test"),
        "TEST_DATABASE_URL must point to a test database (its tables get wiped): {url}"
    );

    let db = Database::connect_to(&url).await.expect("connect + migrate");
    let (client, conn) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    tokio::spawn(conn);
    client
        .batch_execute("TRUNCATE coin_spends, coins, blocks CASCADE")
        .await
        .unwrap();
    (guard, db, client)
}

async fn count(client: &Client, sql: &str) -> i64 {
    client.query_one(sql, &[]).await.unwrap().get(0)
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn insert_block_writes_block_coins_and_spends() {
    let (_guard, mut db, client) = test_db().await;
    let block = load_event(TX_BLOCK);
    let h = TX_BLOCK as i64;

    assert!(db.insert_block(&block).await.unwrap(), "first insert writes the block");
    assert!(!db.insert_block(&block).await.unwrap(), "second insert is a no-op");

    // blocks
    let row = client
        .query_one(
            "SELECT encode(header_hash, 'hex'), fees::text, spend_count, addition_count
             FROM blocks WHERE height = $1",
            &[&h],
        )
        .await
        .unwrap();
    assert_eq!(row.get::<_, String>(0), block.header_hash);
    assert_eq!(row.get::<_, String>(1), "1370285087");
    assert_eq!(row.get::<_, i32>(2), 46);
    assert_eq!(row.get::<_, i32>(3), 4394);

    // coins
    let created = count(&client, &format!("SELECT COUNT(*) FROM coins WHERE confirmed_height = {h}")).await;
    let spent = count(&client, &format!("SELECT COUNT(*) FROM coins WHERE spent_height = {h}")).await;
    let rewards = count(&client, &format!("SELECT COUNT(*) FROM coins WHERE coinbase AND confirmed_height = {h}")).await;
    assert_eq!(created, block.coin_additions.len() as i64);
    assert_eq!(spent, block.coin_removals.len() as i64);
    assert_eq!(rewards, block.coin_additions.iter().filter(|c| c.coinbase).count() as i64);

    // coin_spends
    let spends = count(&client, &format!("SELECT COUNT(*) FROM coin_spends WHERE block_height = {h}")).await;
    assert_eq!(spends, 46);

    // amounts survive the NUMERIC round trip
    let total: String = client
        .query_one(
            "SELECT SUM(amount)::text FROM coins WHERE confirmed_height = $1",
            &[&h],
        )
        .await
        .unwrap()
        .get(0);
    let expected: u128 = block
        .coin_additions
        .iter()
        .map(|c| c.amount.parse::<u128>().unwrap())
        .sum();
    assert_eq!(total, expected.to_string());
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn deleting_a_block_rolls_back_its_coins() {
    // Simulates a reorg: removing a block must remove the coins it created and
    // make the coins it spent unspent again (FK CASCADE / SET NULL).
    let (_guard, mut db, client) = test_db().await;
    let block = load_event(TX_BLOCK);
    let h = TX_BLOCK as i64;
    db.insert_block(&block).await.unwrap();

    client
        .execute("DELETE FROM blocks WHERE height = $1", &[&h])
        .await
        .unwrap();

    assert_eq!(count(&client, &format!("SELECT COUNT(*) FROM coins WHERE confirmed_height = {h}")).await, 0);
    assert_eq!(count(&client, &format!("SELECT COUNT(*) FROM coins WHERE spent_height = {h}")).await, 0);
    assert_eq!(count(&client, &format!("SELECT COUNT(*) FROM coin_spends WHERE block_height = {h}")).await, 0);
}
