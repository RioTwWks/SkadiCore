//! Общие инварианты для property-based тестов парсеров.

use proptest::prelude::*;

/// Непустой ASCII-домен (1..=63 символов), подходит для SOCKS5/VLESS.
pub fn valid_domain() -> impl Strategy<Value = String> {
    prop::collection::vec(
        any::<u8>().prop_filter("label char", |b| {
            b.is_ascii_alphanumeric() || *b == b'.' || *b == b'-'
        }),
        1..64,
    )
    .prop_map(|bytes| String::from_utf8(bytes).unwrap())
}

/// Проверить инварианты `Incomplete { need, have }`.
pub fn assert_incomplete_invariant(have: usize, need: usize, input_len: usize) {
    assert_eq!(have, input_len);
    assert!(have < need);
}

/// На `Ok`: consumed > 0 и consumed <= input.len().
pub fn assert_consumed_in_bounds(consumed: usize, input_len: usize) {
    assert!(consumed > 0, "consumed must be positive");
    assert!(consumed <= input_len, "consumed exceeds input length");
}
