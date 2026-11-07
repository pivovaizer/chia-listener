use crate::types::{BlockReceivedEvent, CoinRecord};
use anyhow::{Context, Result};
use std::collections::HashSet;
use tokio_postgres::{Client, NoTls};
use tracing::{error, info, warn};

// Embeds every file from ./migrations into the binary at compile time.
mod embedded {
    refinery::embed_migrations!("migrations");
}

pub struct Database {
    client: Client,
}

impl Database {
    /// Connects to PostgreSQL using `DATABASE_URL` and applies pending migrations.
    pub async fn connect() -> Result<Self> {
        // .env is optional: DATABASE_URL may also come from the real environment
        dotenvy::dotenv().ok();
        let db_url = std::env::var("DATABASE_URL")
            .context("DATABASE_URL is not set (copy .env-example to .env)")?;
        Self::connect_to(&db_url).await
    }

    /// Connects to the given database and applies pending migrations.
    pub async fn connect_to(db_url: &str) -> Result<Self> {
        let (mut client, connection) = tokio_postgres::connect(db_url, NoTls)
            .await
            .context("cannot connect to PostgreSQL (check that it is running and DATABASE_URL is correct)")?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                error!("DB connection error: {}", e);
            }
        });

        // Apply pending migrations; already applied ones are skipped.
        let report = embedded::migrations::runner()
            .run_async(&mut client)
            .await
            .context("DB migrations failed")?;
        for m in report.applied_migrations() {
            info!("Applied migration {}", m);
        }

        Ok(Database { client })
    }

    // TODO: find_gaps() — scan blocks table for missing heights
    // TODO: backfill missing blocks from peers into DB

    /// Stores a block with its coins and spends in a single transaction.
    ///
    /// Returns `Ok(true)` if the block was written, `Ok(false)` if a block at this
    /// height already exists (duplicate from another peer, or a reorg — not handled yet).
    pub async fn insert_block(&mut self, block: &BlockReceivedEvent) -> Result<bool> {
        let height = block.height as i64;
        let tx = self.client.transaction().await?;

        // --- blocks -------------------------------------------------------
        let prev_tx_hash = block
            .prev_transaction_block_hash
            .as_deref()
            .map(unhex)
            .transpose()?;
        let generator_refs: Vec<i64> = block.generator_ref_list.iter().map(|h| *h as i64).collect();

        let inserted = tx
            .execute(
                "INSERT INTO blocks (
                    height, header_hash, prev_header_hash, weight, total_iters,
                    signage_point_index, is_transaction_block, prev_transaction_block_height,
                    timestamp, farmer_puzzle_hash, pool_puzzle_hash, plot_public_key,
                    pool_contract_puzzle_hash, fees, cost, generator_size, generator_ref_list,
                    spend_count, addition_count, removal_count
                ) VALUES (
                    $1, $2, $3, $4::text::numeric, $5::text::numeric,
                    $6, $7, (SELECT height FROM blocks WHERE header_hash = $8),
                    $9, $10, $11, $12,
                    $13, $14::text::numeric, $15, $16, $17,
                    $18, $19, $20
                )
                ON CONFLICT (height) DO NOTHING",
                &[
                    &height,
                    &unhex(&block.header_hash)?,
                    &unhex(&block.prev_header_hash)?,
                    &block.weight,
                    &block.total_iters,
                    &(block.signage_point_index as i16),
                    &block.is_transaction_block,
                    &prev_tx_hash,
                    &block.timestamp.map(|t| t as i64),
                    &unhex(&block.farmer_puzzle_hash)?,
                    &unhex(&block.pool_puzzle_hash)?,
                    &unhex(&block.plot_public_key)?,
                    &block.pool_contract_puzzle_hash.as_deref().map(unhex).transpose()?,
                    &block.fees.map(|f| f.to_string()),
                    &block.cost.map(|c| c as i64),
                    &(block.generator_size as i32),
                    &generator_refs,
                    &(block.coin_spends.len() as i32),
                    &(block.coin_additions.len() as i32),
                    &(block.coin_removals.len() as i32),
                ],
            )
            .await
            .context("insert into blocks")?;

        if inserted == 0 {
            // Same height already stored. Same hash = duplicate (normal); different hash = reorg.
            let row = tx
                .query_one("SELECT header_hash FROM blocks WHERE height = $1", &[&height])
                .await?;
            let stored: Vec<u8> = row.get(0);
            if hex::encode(stored) != block.header_hash {
                warn!(
                    "Block {} has a different header hash than the stored one (reorg?) — skipped",
                    block.height
                );
            }
            return Ok(false);
        }

        let timestamp = block.timestamp.map(|t| t as i64);

        // --- coins: additions (created in this block) -----------------------
        // Unique by coin_id: ON CONFLICT DO UPDATE cannot touch the same row twice.
        let additions = Coins::from_records(&block.coin_additions)?;
        tx.execute(
            "INSERT INTO coins (coin_id, parent_coin_info, puzzle_hash, amount,
                                confirmed_height, coinbase, timestamp)
             SELECT c.id, c.parent, c.ph, c.amount::numeric, $5, c.cb, $6
             FROM UNNEST($1::bytea[], $2::bytea[], $3::bytea[], $4::text[], $7::bool[])
                  AS c(id, parent, ph, amount, cb)
             ON CONFLICT (coin_id) DO UPDATE
                SET confirmed_height = EXCLUDED.confirmed_height,
                    coinbase         = EXCLUDED.coinbase,
                    timestamp        = EXCLUDED.timestamp
                WHERE coins.confirmed_height IS NULL",
            &[
                &additions.ids,
                &additions.parents,
                &additions.puzzle_hashes,
                &additions.amounts,
                &height,
                &timestamp,
                &additions.coinbase,
            ],
        )
        .await
        .context("insert coin additions")?;

        // --- coins: removals (spent in this block) --------------------------
        // A coin created before our sync started is inserted with confirmed_height = NULL.
        let removals = Coins::from_records(&block.coin_removals)?;
        tx.execute(
            "INSERT INTO coins (coin_id, parent_coin_info, puzzle_hash, amount, spent_height)
             SELECT c.id, c.parent, c.ph, c.amount::numeric, $5
             FROM UNNEST($1::bytea[], $2::bytea[], $3::bytea[], $4::text[])
                  AS c(id, parent, ph, amount)
             ON CONFLICT (coin_id) DO UPDATE SET spent_height = EXCLUDED.spent_height",
            &[
                &removals.ids,
                &removals.parents,
                &removals.puzzle_hashes,
                &removals.amounts,
                &height,
            ],
        )
        .await
        .context("insert coin removals")?;

        // --- coin_spends ----------------------------------------------------
        let mut spend_ids = Vec::with_capacity(block.coin_spends.len());
        let mut puzzles = Vec::with_capacity(block.coin_spends.len());
        let mut solutions = Vec::with_capacity(block.coin_spends.len());
        let mut offsets = Vec::with_capacity(block.coin_spends.len());
        for spend in &block.coin_spends {
            spend_ids.push(unhex(&spend.coin.coin_id)?);
            puzzles.push(unhex(&spend.puzzle_reveal)?);
            solutions.push(unhex(&spend.solution)?);
            offsets.push(spend.offset as i32);
        }
        tx.execute(
            "INSERT INTO coin_spends (coin_id, block_height, puzzle_reveal, solution, generator_offset)
             SELECT s.id, $1, s.puzzle, s.solution, s.off
             FROM UNNEST($2::bytea[], $3::bytea[], $4::bytea[], $5::int4[])
                  AS s(id, puzzle, solution, off)
             ON CONFLICT (coin_id) DO NOTHING",
            &[&height, &spend_ids, &puzzles, &solutions, &offsets],
        )
        .await
        .context("insert coin spends")?;

        tx.commit().await?;
        Ok(true)
    }

    /// Highest stored block height, or `None` if the table is empty.
    pub async fn get_max_height(&self) -> Result<Option<i64>> {
        let row = self
            .client
            .query_one("SELECT MAX(height) FROM blocks", &[])
            .await
            .context("query max block height")?;
        Ok(row.get(0))
    }
}

/// Decodes a lowercase hex hash into raw bytes for BYTEA columns.
fn unhex(s: &str) -> Result<Vec<u8>> {
    hex::decode(s).with_context(|| format!("invalid hex: {s}"))
}

/// Column arrays for a batch of coins, passed to Postgres via UNNEST.
struct Coins {
    ids: Vec<Vec<u8>>,
    parents: Vec<Vec<u8>>,
    puzzle_hashes: Vec<Vec<u8>>,
    amounts: Vec<String>,
    coinbase: Vec<bool>,
}

impl Coins {
    /// Builds column arrays, skipping duplicate coin ids.
    fn from_records(records: &[CoinRecord]) -> Result<Self> {
        let mut seen = HashSet::with_capacity(records.len());
        let mut coins = Coins {
            ids: Vec::with_capacity(records.len()),
            parents: Vec::with_capacity(records.len()),
            puzzle_hashes: Vec::with_capacity(records.len()),
            amounts: Vec::with_capacity(records.len()),
            coinbase: Vec::with_capacity(records.len()),
        };
        for coin in records {
            if !seen.insert(coin.coin_id.as_str()) {
                continue;
            }
            coins.ids.push(unhex(&coin.coin_id)?);
            coins.parents.push(unhex(&coin.parent_coin_info)?);
            coins.puzzle_hashes.push(unhex(&coin.puzzle_hash)?);
            coins.amounts.push(coin.amount.clone());
            coins.coinbase.push(coin.coinbase);
        }
        Ok(coins)
    }
}
