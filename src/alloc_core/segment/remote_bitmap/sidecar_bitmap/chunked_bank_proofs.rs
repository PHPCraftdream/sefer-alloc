//! Последовательная арифметика production helpers; не доказательство memory ordering.

use super::{ChunkedBank, CHUNKS, MIN_BLOCK, READY_WORDS, SEGMENT, WORDS, WORDS_PER_CHUNK};

#[kani::proof]
fn chunked_granule_rejects_every_invalid_offset() {
    let offset: u32 = kani::any();
    let got = ChunkedBank::granule(offset);
    let valid = (offset as usize) < SEGMENT && (offset as usize) % MIN_BLOCK == 0;
    assert_eq!(got.is_some(), valid);
    if let Some(granule) = got {
        assert_eq!(granule * MIN_BLOCK, offset as usize);
        assert!(granule < ChunkedBank::GRANULES);
    }
    kani::cover!(valid);
    kani::cover!(!valid);
}

#[kani::proof]
fn chunked_word_location_stays_inside_backing_and_ready() {
    let word: usize = kani::any();
    kani::assume(word < WORDS);
    let (chunk, within) = ChunkedBank::checked_word(word);
    assert!(chunk < CHUNKS);
    assert!(within < WORDS_PER_CHUNK);
    assert!(chunk / 64 < READY_WORDS);
    assert_eq!(chunk * WORDS_PER_CHUNK + within, word);
    kani::cover!(word == WORDS - 1);
}

#[kani::proof]
fn chunked_issue_and_publish_share_exact_bit_location() {
    let offset: u32 = kani::any();
    let Some((chunk, word, mask)) = ChunkedBank::location(offset) else {
        assert!(ChunkedBank::granule(offset).is_none());
        return;
    };
    assert_eq!(mask.count_ones(), 1);
    let flat = (chunk * WORDS_PER_CHUNK + word) * 64 + mask.trailing_zeros() as usize;
    assert_eq!(flat * MIN_BLOCK, offset as usize);
    assert!(chunk < CHUNKS && word < WORDS_PER_CHUNK);
    kani::cover!(word == WORDS_PER_CHUNK - 1 && mask == 1u64 << 63);
}

#[kani::proof]
fn chunked_scan_validates_original_bounds_including_empty_window() {
    let high_water: usize = kani::any();
    let word: usize = kani::any();
    let got = ChunkedBank::window_end(high_water, word);
    let bytes = 64 * MIN_BLOCK;
    let expected_end = high_water / bytes + usize::from(high_water % bytes != 0);
    assert_eq!(got.is_some(), high_water <= SEGMENT && word <= expected_end);
    if let Some(end) = got {
        assert!(word <= end && end <= WORDS);
        assert!(end * bytes >= high_water);
        if end > 0 {
            assert!((end - 1) * bytes < high_water);
        }
    }
    kani::cover!(high_water == 0 && word == 0 && got == Some(0));
    kani::cover!(high_water > SEGMENT && got.is_none());
}

#[kani::proof]
fn chunked_range_mask_contains_exactly_the_half_open_interval() {
    let start: usize = kani::any();
    let end: usize = kani::any();
    let word: usize = kani::any();
    let bit: u32 = kani::any();
    kani::assume(start <= end && end <= ChunkedBank::GRANULES);
    kani::assume(word < WORDS && bit < 64);
    let mask = ChunkedBank::range_mask(start, end, word);
    let granule = word * 64 + bit as usize;
    assert_eq!(mask & (1u64 << bit) != 0, granule >= start && granule < end);
    kani::cover!(mask == u64::MAX);
    kani::cover!(mask == 0);
}
