//! Unit tests for coin ids and parser error handling.

use crate::*;
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// coin_id
// ---------------------------------------------------------------------------

const PARENT: [u8; 32] = [0x11; 32];
const PUZZLE: [u8; 32] = [0x22; 32];

/// Independent reference implementation of the Chia coin id:
/// sha256(parent || puzzle_hash || amount), where amount is encoded as a
/// minimal big-endian CLVM integer (no leading zeros, 0x00 prefix if the
/// top bit is set, empty for 0).
fn reference_coin_id(parent: &[u8; 32], puzzle: &[u8; 32], amount: u64) -> String {
    let be = amount.to_be_bytes();
    let first_nonzero = be.iter().position(|b| *b != 0).unwrap_or(be.len());
    let mut amount_bytes = be[first_nonzero..].to_vec();
    if amount_bytes.first().is_some_and(|b| b & 0x80 != 0) {
        amount_bytes.insert(0, 0);
    }

    let mut hasher = Sha256::new();
    hasher.update(parent);
    hasher.update(puzzle);
    hasher.update(&amount_bytes);
    hex::encode(hasher.finalize())
}

#[test]
fn coin_id_matches_reference_for_edge_amounts() {
    // 0, sign-bit boundaries, typical rewards (1.75 / 0.25 XCH) and u64::MAX
    let amounts = [
        0,
        1,
        0x7f,
        0x80,
        0xff,
        0x7fff,
        0x8000,
        1_750_000_000_000,
        250_000_000_000,
        0x7fff_ffff_ffff_ffff,
        0x8000_0000_0000_0000,
        u64::MAX,
    ];
    for amount in amounts {
        let coin = CoinInfo::new(Bytes32::from(PARENT), Bytes32::from(PUZZLE), amount);
        assert_eq!(
            coin.coin_id,
            reference_coin_id(&PARENT, &PUZZLE, amount),
            "coin_id mismatch for amount {amount}"
        );
    }
}

#[test]
fn coin_info_fields_are_lowercase_hex() {
    let coin = CoinInfo::new(Bytes32::from(PARENT), Bytes32::from(PUZZLE), 42);
    assert_eq!(coin.coin_id.len(), 64);
    assert_eq!(coin.parent_coin_info, "11".repeat(32));
    assert_eq!(coin.puzzle_hash, "22".repeat(32));
    assert_eq!(coin.amount, 42);
    assert!(coin.coin_id.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)));
}

#[test]
fn coinbase_flag() {
    let coin = CoinInfo::new(Bytes32::from(PARENT), Bytes32::from(PUZZLE), 1);
    assert!(!coin.coinbase, "coins are not coinbase by default");
    let id = coin.coin_id.clone();
    let reward = coin.into_coinbase();
    assert!(reward.coinbase);
    assert_eq!(reward.coin_id, id, "coinbase flag must not change the coin id");
}

#[test]
fn from_bytes_matches_new_and_rejects_bad_lengths() {
    let via_bytes = CoinInfo::from_bytes(&PARENT, &PUZZLE, 7).expect("valid 32-byte hashes");
    let via_new = CoinInfo::new(Bytes32::from(PARENT), Bytes32::from(PUZZLE), 7);
    assert_eq!(via_bytes.coin_id, via_new.coin_id);

    assert!(CoinInfo::from_bytes(&PARENT[..31], &PUZZLE, 7).is_none());
    assert!(CoinInfo::from_bytes(&PARENT, &[0u8; 33], 7).is_none());
}

// ---------------------------------------------------------------------------
// process_generator_for_coins: errors must never become an "empty block"
// ---------------------------------------------------------------------------

#[test]
fn empty_generator_yields_no_coins() {
    let parser = BlockParser::new();
    let (removals, spends, creations) = parser
        .process_generator_for_coins(&[], &[], 100)
        .expect("empty generator is valid");
    assert!(removals.is_empty() && spends.is_empty() && creations.is_empty());
}

#[test]
fn generator_refs_are_rejected_not_parsed_as_empty() {
    let parser = BlockParser::new();
    // Any non-empty generator with references must be rejected up front
    let err = parser
        .process_generator_for_coins(&[0xff, 0x01, 0x80], &[123, 456], 100)
        .expect_err("blocks with generator refs are not supported yet");
    match err {
        GeneratorParserError::UnsupportedGeneratorRefs { height, refs } => {
            assert_eq!(height, 100);
            assert_eq!(refs, vec![123, 456]);
        }
        other => panic!("expected UnsupportedGeneratorRefs, got {other:?}"),
    }
}

#[test]
fn garbage_generator_is_an_error() {
    let parser = BlockParser::new();
    // 0xff starts a cons cell with no children: truncated / invalid CLVM
    let result = parser.process_generator_for_coins(&[0xff], &[], 100);
    assert!(result.is_err(), "invalid bytecode must be an error, got {result:?}");
}

#[test]
fn generator_without_spend_list_is_an_error() {
    let parser = BlockParser::new();
    // 0x80 is nil: a valid program, but its output is not a (spends) list
    let result = parser.process_generator_for_coins(&[0x80], &[], 100);
    assert!(result.is_err(), "unexpected generator output must be an error, got {result:?}");
}

#[test]
fn generator_with_zero_spends_is_ok() {
    let parser = BlockParser::new();
    // (q . (())) = ff01ff8080: returns a list whose first element is an empty spend list
    let (removals, spends, creations) = parser
        .process_generator_for_coins(&hex::decode("ff01ff8080").unwrap(), &[], 100)
        .expect("a generator with an empty spend list is valid");
    assert!(removals.is_empty() && spends.is_empty() && creations.is_empty());
}
