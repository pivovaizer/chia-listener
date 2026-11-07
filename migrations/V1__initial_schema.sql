-- V1: initial schema.
--
-- Migrations are applied automatically by the listener on startup (refinery).
-- NEVER edit a migration that has already been applied: add a new file
-- V<N+1>__<description>.sql instead. Applied versions are tracked in
-- the `refinery_schema_history` table.
--
-- Conventions:
--   * Hashes (header_hash, coin_id, puzzle_hash, ...) are BYTEA, 32 raw bytes.
--     Half the size of hex TEXT, which matters for indexes on millions of rows.
--     In psql:  encode(coin_id, 'hex')   /   WHERE coin_id = decode('ab12...', 'hex')
--   * Amounts are mojo (1 XCH = 10^12 mojo). NUMERIC(20,0) holds the full u64 range,
--     which BIGINT (signed i64) does not.
--   * Heights are BIGINT (Chia uses u32; BIGINT avoids sign issues).

-- ---------------------------------------------------------------------------
-- blocks: one row per block in the canonical chain
-- ---------------------------------------------------------------------------
CREATE TABLE blocks (
    height                         BIGINT        PRIMARY KEY,
    header_hash                    BYTEA         NOT NULL UNIQUE,
    prev_header_hash               BYTEA         NOT NULL,   -- foliage.prev_block_hash; used for reorg detection
    weight                         NUMERIC(39,0) NOT NULL,   -- u128
    total_iters                    NUMERIC(39,0) NOT NULL,   -- u128
    signage_point_index            SMALLINT      NOT NULL,
    is_transaction_block           BOOLEAN       NOT NULL,
    prev_transaction_block_height  BIGINT,                   -- previous transaction block (navigation)
    timestamp                      BIGINT,                   -- unix seconds; transaction blocks only

    -- Who farmed the block
    farmer_puzzle_hash             BYTEA         NOT NULL,   -- farmer reward target
    pool_puzzle_hash               BYTEA         NOT NULL,   -- pool reward target
    plot_public_key                BYTEA         NOT NULL,   -- proof_of_space.plot_public_key (48 bytes)
    pool_contract_puzzle_hash      BYTEA,                    -- proof_of_space; set for pooling (NFT) plots

    -- Transactions (transaction blocks only)
    fees                           NUMERIC(20,0),            -- user fees in mojo
    cost                           BIGINT,                   -- CLVM cost
    generator_size                 INTEGER       NOT NULL DEFAULT 0,
    generator_ref_list             BIGINT[]      NOT NULL DEFAULT '{}',  -- heights of referenced generators

    -- Denormalized counters for block lists
    spend_count                    INTEGER       NOT NULL DEFAULT 0,
    addition_count                 INTEGER       NOT NULL DEFAULT 0,
    removal_count                  INTEGER       NOT NULL DEFAULT 0,

    received_at                    TIMESTAMPTZ   NOT NULL DEFAULT now()
);

CREATE INDEX idx_blocks_timestamp ON blocks(timestamp);
CREATE INDEX idx_blocks_farmer    ON blocks(farmer_puzzle_hash);

-- ---------------------------------------------------------------------------
-- coins: coin records, one row per coin ever created.
-- A coin is created once (confirmed_height) and spent at most once (spent_height).
-- ---------------------------------------------------------------------------
CREATE TABLE coins (
    coin_id               BYTEA         PRIMARY KEY,       -- sha256(parent_coin_info || puzzle_hash || amount)
    parent_coin_info      BYTEA         NOT NULL,
    puzzle_hash           BYTEA         NOT NULL,          -- address = bech32m("xch", puzzle_hash)
    amount                NUMERIC(20,0) NOT NULL,

    -- NULL = created before our sync started (we saw only its spend). Backfill fills it in.
    confirmed_height      BIGINT        REFERENCES blocks(height) ON DELETE CASCADE,
    -- NULL = unspent. On reorg the spending block is deleted and the coin becomes unspent again.
    spent_height          BIGINT        REFERENCES blocks(height) ON DELETE SET NULL,

    coinbase              BOOLEAN       NOT NULL DEFAULT FALSE,  -- farmer / pool reward
    timestamp             BIGINT                           -- timestamp of the confirming block
);

CREATE INDEX idx_coins_puzzle_hash      ON coins(puzzle_hash);       -- address lookup
CREATE INDEX idx_coins_parent           ON coins(parent_coin_info);  -- coin lineage
CREATE INDEX idx_coins_confirmed_height ON coins(confirmed_height);
CREATE INDEX idx_coins_spent_height     ON coins(spent_height);
-- Fast "unspent balance by address"
CREATE INDEX idx_coins_unspent_by_puzzle ON coins(puzzle_hash) WHERE spent_height IS NULL;

-- ---------------------------------------------------------------------------
-- coin_spends: how a coin was spent (the puzzle and the arguments it ran with).
-- No FK to coins: a spent coin may predate the sync start and be missing from `coins`.
-- ---------------------------------------------------------------------------
CREATE TABLE coin_spends (
    coin_id               BYTEA         PRIMARY KEY,       -- a coin can be spent only once
    block_height          BIGINT        NOT NULL REFERENCES blocks(height) ON DELETE CASCADE,
    puzzle_reveal         BYTEA         NOT NULL,          -- serialized CLVM program
    solution              BYTEA         NOT NULL,          -- serialized CLVM program
    generator_offset      INTEGER       NOT NULL           -- position of the spend in the block generator
);

CREATE INDEX idx_spends_block ON coin_spends(block_height);

-- ---------------------------------------------------------------------------
-- sync_state: key-value store for sync progress
-- (e.g. 'backfill_cursor' -> '1234567'). Used by gap detection / backfill.
-- ---------------------------------------------------------------------------
CREATE TABLE sync_state (
    key                   TEXT          PRIMARY KEY,
    value                 TEXT          NOT NULL,
    updated_at            TIMESTAMPTZ   NOT NULL DEFAULT now()
);
