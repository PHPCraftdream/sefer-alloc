//! Fixed scalar event encoding; syntax/geometry checks do not authenticate events.
#![forbid(unsafe_code)]

/// Inert diagnostic event. No integer is converted to a pointer or dereferenced.
/// Modes are Off=0, Always=1, Auto=2; groups use requested-byte MiB boundaries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdviceEvent {
    words: [u64; 7],
}

impl AdviceEvent {
    pub const ENCODING_VERSION: u32 = 1;
    pub const FRESH: u8 = 1;
    pub const ACCEPTED: u8 = 2;
    pub const RELEASED: u8 = 3;
    pub const UNKNOWN_ADVICE: u8 = 4;
    pub const TAG: usize = 0;
    pub const RESERVATION: usize = 1;
    pub const RESERVATION_LEN: usize = 2;
    pub const USABLE: usize = 3;
    pub const USABLE_LEN: usize = 4;
    pub const REQUESTED: usize = 5;
    /// Fresh: configured minimum; accepted/unknown: actual/attempted advice bytes.
    pub const VALUE: usize = 6;
    const THRESHOLD_ELIGIBLE: u64 = 1 << 8;
    const CONFIRMED: u64 = 1 << 9;
    const RELEASE_MARKED: u64 = 1 << 10;
    const LARGE_RELEASE: u64 = 1 << 11;
    pub const RELEASE_BEGIN: usize = 3;

    fn group_of(requested: u64) -> u64 {
        match requested / (1024 * 1024) {
            0 => 0,
            1..=7 => 1,
            8..=31 => 2,
            32..=127 => 3,
            128..=511 => 4,
            _ => 5,
        }
    }

    fn mapping(values: [usize; 6], kind: u8, mode: u8) -> Result<Self, &'static str> {
        let [reservation, reservation_len, usable, usable_len, requested, value] = values;
        let end = reservation
            .checked_add(reservation_len)
            .ok_or("event reservation overflow")?;
        let usable_end = usable
            .checked_add(usable_len)
            .ok_or("event usable span overflow")?;
        if reservation == 0
            || reservation_len == 0
            || usable_len == 0
            || requested == 0
            || usable < reservation
            || usable_end > end
            || requested > usable_len
            || mode > 2
        {
            return Err("invalid advice event geometry or mode");
        }
        if kind != Self::FRESH && (mode == 0 || value == 0 || value > usable_len) {
            return Err("invalid advised event extent or mode");
        }
        let mut words = [0; 7];
        for (destination, value) in words[1..].iter_mut().zip(values) {
            *destination = u64::try_from(value).map_err(|_| "event integer conversion overflow")?;
        }
        words[0] = u64::from(kind)
            | (Self::group_of(words[Self::REQUESTED]) << 3)
            | (u64::from(mode) << 6);
        // Threshold eligibility is separate from mode: Off never emits acceptance.
        if kind == Self::FRESH && requested >= value {
            words[0] |= Self::THRESHOLD_ELIGIBLE;
        }
        Ok(Self { words })
    }

    pub fn fresh(
        reservation: usize,
        reservation_len: usize,
        usable: usize,
        usable_len: usize,
        requested: usize,
        minimum: usize,
        mode: u8,
    ) -> Result<Self, &'static str> {
        Self::mapping(
            [
                reservation,
                reservation_len,
                usable,
                usable_len,
                requested,
                minimum,
            ],
            Self::FRESH,
            mode,
        )
    }

    pub fn accepted(
        reservation: usize,
        reservation_len: usize,
        usable: usize,
        usable_len: usize,
        requested: usize,
        advised: usize,
        mode: u8,
    ) -> Result<Self, &'static str> {
        Self::mapping(
            [
                reservation,
                reservation_len,
                usable,
                usable_len,
                requested,
                advised,
            ],
            Self::ACCEPTED,
            mode,
        )
    }

    pub fn unknown_advice(
        reservation: usize,
        reservation_len: usize,
        usable: usize,
        usable_len: usize,
        requested: usize,
        attempted: usize,
        mode: u8,
    ) -> Result<Self, &'static str> {
        Self::mapping(
            [
                reservation,
                reservation_len,
                usable,
                usable_len,
                requested,
                attempted,
            ],
            Self::UNKNOWN_ADVICE,
            mode,
        )
    }

    pub fn released(
        reservation: usize,
        reservation_len: usize,
        confirmed: bool,
    ) -> Result<Self, &'static str> {
        if reservation == 0
            || reservation_len == 0
            || reservation.checked_add(reservation_len).is_none()
        {
            return Err("invalid release event geometry");
        }
        Ok(Self {
            words: [
                u64::from(Self::RELEASED) | if confirmed { Self::CONFIRMED } else { 0 },
                u64::try_from(reservation).map_err(|_| "event address conversion overflow")?,
                u64::try_from(reservation_len).map_err(|_| "event length conversion overflow")?,
                0,
                0,
                0,
                0,
            ],
        })
    }

    /// Native release marker is sampled before the consuming VM attempt. It is
    /// an event-position hint, not a timestamp or permission to retry a release.
    pub fn released_at(
        reservation: usize,
        reservation_len: usize,
        confirmed: bool,
        begin: u64,
    ) -> Result<Self, &'static str> {
        if begin > 65536 {
            return Err("release marker exceeds capture bound");
        }
        let mut event = Self::released(reservation, reservation_len, confirmed)?;
        event.words[0] |= Self::RELEASE_MARKED;
        event.words[Self::RELEASE_BEGIN] = begin;
        Ok(event)
    }

    pub fn large_released_at(
        reservation: usize,
        len: usize,
        confirmed: bool,
        begin: u64,
    ) -> Result<Self, &'static str> {
        let mut event = Self::released_at(reservation, len, confirmed, begin)?;
        event.words[0] |= Self::LARGE_RELEASE;
        Ok(event)
    }

    pub const fn large_release(self) -> bool {
        self.words[0] & Self::LARGE_RELEASE != 0
    }

    pub fn release_begin(self) -> Option<u64> {
        if self.kind() == Self::RELEASED && self.words[0] & Self::RELEASE_MARKED != 0 {
            Some(self.words[Self::RELEASE_BEGIN])
        } else {
            None
        }
    }

    /// Strict inverse of `words`: reserved bits, unused fields and false group or
    /// eligibility labels reject rather than normalizing a malformed event.
    pub fn from_words(words: [u64; 7]) -> Result<Self, &'static str> {
        let mut values = [0usize; 6];
        for (value, encoded) in values.iter_mut().zip(&words[1..]) {
            *value = usize::try_from(*encoded).map_err(|_| "event exceeds native integer width")?;
        }
        let kind = u8::try_from(words[0] & 7).map_err(|_| "invalid event kind")?;
        let mode = u8::try_from((words[0] >> 6) & 3).map_err(|_| "invalid event mode")?;
        let canonical = match kind {
            Self::FRESH | Self::ACCEPTED | Self::UNKNOWN_ADVICE => {
                Self::mapping(values, kind, mode)?
            }
            Self::RELEASED => {
                if words[0] & Self::RELEASE_MARKED != 0 {
                    if words[0] & Self::LARGE_RELEASE != 0 {
                        Self::large_released_at(
                            values[0],
                            values[1],
                            words[0] & Self::CONFIRMED != 0,
                            words[Self::RELEASE_BEGIN],
                        )?
                    } else {
                        Self::released_at(
                            values[0],
                            values[1],
                            words[0] & Self::CONFIRMED != 0,
                            words[Self::RELEASE_BEGIN],
                        )?
                    }
                } else {
                    Self::released(values[0], values[1], words[0] & Self::CONFIRMED != 0)?
                }
            }
            _ => return Err("unknown advice event tag"),
        };
        if canonical.words != words {
            return Err("noncanonical advice event encoding");
        }
        Ok(canonical)
    }

    pub const fn words(self) -> [u64; 7] {
        self.words
    }
    pub fn kind(self) -> u8 {
        u8::try_from(self.words[0] & 7).expect("validated three-bit tag")
    }
    pub fn group(self) -> Option<usize> {
        if self.kind() == Self::RELEASED {
            None
        } else {
            Some(usize::try_from((self.words[0] >> 3) & 7).expect("validated three-bit group"))
        }
    }
    pub fn mode(self) -> Option<u8> {
        if self.kind() == Self::RELEASED {
            None
        } else {
            Some(u8::try_from((self.words[0] >> 6) & 3).expect("validated two-bit mode"))
        }
    }
    pub const fn threshold_eligible(self) -> bool {
        self.words[0] & Self::THRESHOLD_ELIGIBLE != 0
    }
    pub const fn confirmed_release(self) -> bool {
        self.words[0] & Self::CONFIRMED != 0
    }
}
