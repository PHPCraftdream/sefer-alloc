//! R2: [`LiveTag`] — the LIVE state byte of a small class, typed so its value
//! range is known to the compiler. `tag as u8 == StateByte::encode(c, LIVE)`
//! and `tag.class() == c`; a load from [`LiveTag::of_seed`]'s table carries
//! the range `1..=SMALL_CLASS_COUNT`, so the class bound and the magazine
//! index need no separate check.

use crate::alloc_core::size_classes::{S2C_LEN, SIZE2CLASS, SMALL_CLASS_COUNT};
use crate::alloc_core::state_byte::StateByte;

macro_rules! live_tags {
    ($($(#[$m:meta])* $v:ident = $n:literal,)*) => {
        /// `encode(c, LIVE) == c + 1` for every small class of this build.
        #[repr(u8)]
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub(crate) enum LiveTag {
            $($(#[$m])* $v = $n,)*
        }

        impl LiveTag {
            /// Number of variants in this build.
            const COUNT: usize = {
                let mut n = 0;
                $($(#[$m])* { n += 1; })*
                n
            };

            const fn from_class(c: usize) -> Self {
                match c + 1 {
                    $($(#[$m])* $n => LiveTag::$v,)*
                    _ => panic!("LiveTag: class out of range"),
                }
            }
        }
    };
}

live_tags! {
    T1 = 1, T2 = 2, T3 = 3, T4 = 4, T5 = 5, T6 = 6, T7 = 7, T8 = 8, T9 = 9, T10 = 10,
    T11 = 11, T12 = 12, T13 = 13, T14 = 14, T15 = 15, T16 = 16, T17 = 17, T18 = 18,
    T19 = 19, T20 = 20, T21 = 21, T22 = 22, T23 = 23, T24 = 24, T25 = 25, T26 = 26,
    T27 = 27, T28 = 28, T29 = 29, T30 = 30, T31 = 31, T32 = 32, T33 = 33, T34 = 34,
    T35 = 35, T36 = 36, T37 = 37, T38 = 38, T39 = 39, T40 = 40, T41 = 41, T42 = 42,
    T43 = 43, T44 = 44, T45 = 45, T46 = 46, T47 = 47, T48 = 48, T49 = 49,
    #[cfg(feature = "medium-classes")] T50 = 50,
    #[cfg(feature = "medium-classes")] T51 = 51,
    #[cfg(feature = "medium-classes")] T52 = 52,
    #[cfg(feature = "medium-classes")] T53 = 53,
    #[cfg(feature = "medium-classes")] T54 = 54,
    #[cfg(feature = "medium-classes")] T55 = 55,
    #[cfg(feature = "medium-classes-wide")] T56 = 56,
    #[cfg(feature = "medium-classes-wide")] T57 = 57,
    #[cfg(feature = "medium-classes-wide")] T58 = 58,
}

// The variants are exactly `1..=SMALL_CLASS_COUNT`: no class is missing
// (`from_class` would fail below) and none is extra.
const _: () = assert!(LiveTag::COUNT == SMALL_CLASS_COUNT);

/// `SIZE2CLASS` as tags: one entry per `MIN_BLOCK` bucket, same index.
static SIZE2TAG: [LiveTag; S2C_LEN] = {
    let mut t = [LiveTag::T1; S2C_LEN];
    let mut i = 0;
    while i < S2C_LEN {
        t[i] = LiveTag::from_class(SIZE2CLASS[i] as usize);
        i += 1;
    }
    t
};

impl LiveTag {
    /// The tag of bucket `seed = (size - 1) >> MIN_BLOCK_SHIFT`; the caller
    /// has bounded `seed < S2C_LEN - 1`.
    #[inline(always)]
    pub(crate) const fn of_seed(seed: usize) -> Self {
        SIZE2TAG[seed]
    }

    /// The class index.
    #[inline(always)]
    pub(crate) const fn class(self) -> usize {
        self as usize - 1
    }

    /// The LIVE state byte (read by the AS-1 free lane only).
    #[cfg_attr(
        not(all(
            feature = "alloc-global",
            feature = "fastbin",
            not(feature = "hardened")
        )),
        allow(dead_code)
    )]
    #[inline(always)]
    pub(crate) const fn byte(self) -> u8 {
        self as u8
    }

    /// The MAG state byte of the same class (`encode(c, MAG)`).
    #[cfg_attr(
        not(all(
            feature = "alloc-global",
            feature = "fastbin",
            not(feature = "hardened")
        )),
        allow(dead_code)
    )]
    #[inline(always)]
    pub(crate) const fn mag_byte(self) -> u8 {
        self as u8 | (StateByte::MAG << 6)
    }
}

// `encode(c, s) == (c + 1) | (s << 6)`: the tag is the class field.
const _: () = {
    let mut c = 0;
    while c < SMALL_CLASS_COUNT {
        let t = LiveTag::from_class(c);
        assert!(t.class() == c);
        assert!(t as u8 == StateByte::encode(c, StateByte::LIVE));
        assert!(t as u8 | (StateByte::MAG << 6) == StateByte::encode(c, StateByte::MAG));
        c += 1;
    }
};
