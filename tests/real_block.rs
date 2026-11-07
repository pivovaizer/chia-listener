//! Parses real mainnet blocks saved in tests/fixtures/ and checks them against
//! values verified on a public explorer plus chain invariants.
//!
//! Fixtures are created with:
//!   cargo run --example dump_block -- 9540781 9540780

use chia_generator_parser::{BlockParser, ParsedBlock};
use chia_protocol::FullBlock;
use chia_traits::Streamable;
use std::collections::HashSet;

/// Transaction block, verified on an explorer: fees 0.001370285087 XCH
const TX_BLOCK: u32 = 9540781;
/// Non-transaction block
const NON_TX_BLOCK: u32 = 9540780;

fn parse_fixture(height: u32) -> ParsedBlock {
    let path = format!("tests/fixtures/block_{height}.bin");
    let bytes = std::fs::read(&path).unwrap_or_else(|_| {
        panic!("missing {path}; create it with: cargo run --example dump_block -- {height}")
    });
    let block = FullBlock::from_bytes(&bytes).expect("fixture is a valid FullBlock");
    BlockParser::new().parse_full_block(&block).expect("block parses")
}

fn sum(amounts: impl Iterator<Item = u64>) -> u128 {
    amounts.map(u128::from).sum()
}

#[test]
fn transaction_block_matches_explorer() {
    let b = parse_fixture(TX_BLOCK);

    assert_eq!(b.height, TX_BLOCK);
    assert!(b.is_transaction_block);
    assert!(b.timestamp.is_some());
    assert_eq!(b.fees, Some(1_370_285_087));
    assert_eq!(b.coin_spends.len(), 46);
    assert_eq!(b.coin_removals.len(), 46);
    assert_eq!(b.coin_additions.len(), 4394);
}

#[test]
fn transaction_block_invariants() {
    let b = parse_fixture(TX_BLOCK);

    // Coin ids are unique
    let addition_ids: HashSet<_> = b.coin_additions.iter().map(|c| &c.coin_id).collect();
    assert_eq!(addition_ids.len(), b.coin_additions.len(), "duplicate addition coin ids");

    // Additions = reward coins + coins created by spends
    let rewards = b.coin_additions.iter().filter(|c| c.coinbase).count();
    assert!(rewards >= 2, "a transaction block includes at least farmer + pool rewards");
    assert_eq!(b.coin_additions.len(), b.coin_creations.len() + rewards);
    assert!(b.coin_creations.iter().all(|c| !c.coinbase), "created coins are not rewards");

    // Every created coin's parent is a coin spent in this block
    let spent_ids: HashSet<_> = b.coin_spends.iter().map(|s| &s.coin.coin_id).collect();
    for coin in &b.coin_creations {
        assert!(
            spent_ids.contains(&coin.parent_coin_info),
            "created coin {} has a parent that was not spent in this block",
            coin.coin_id
        );
    }

    // Spend offsets are the positions 0..n
    let offsets: Vec<u32> = b.coin_spends.iter().map(|s| s.offset).collect();
    assert_eq!(offsets, (0..b.coin_spends.len() as u32).collect::<Vec<_>>());

    // Value conservation: spent = created + fees
    let spent = sum(b.coin_removals.iter().map(|c| c.amount));
    let created = sum(b.coin_creations.iter().map(|c| c.amount));
    assert_eq!(spent, created + u128::from(b.fees.unwrap()), "spent != created + fees");
}

#[test]
fn non_transaction_block_has_no_transaction_data() {
    let b = parse_fixture(NON_TX_BLOCK);

    assert_eq!(b.height, NON_TX_BLOCK);
    assert!(!b.is_transaction_block);
    assert_eq!(b.timestamp, None);
    assert_eq!(b.fees, None);
    assert_eq!(b.cost, None);
    assert!(b.coin_additions.is_empty() && b.coin_removals.is_empty() && b.coin_spends.is_empty());
}
