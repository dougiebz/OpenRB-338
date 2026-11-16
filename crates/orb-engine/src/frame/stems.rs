//! Not in the original: the stems, a copy of what each mixer channel and the delay add to the
//! master, for a host that wants the instruments on separate outputs (a plugin's per-device
//! outputs). They are off unless [`super::Engine::set_stems`] turns them on, and they only read
//! the mixer block: the stereo mix is the same with and without them.
//!
//! A stem is stereo and holds the source's part of the mix: for a channel `bus * gain` per side
//! (after its inserts, its level and its pan, the products the mix loop at 0x10013742 adds), for
//! the delay what its slot 14 added to the master (0x10013844), each times the block's master
//! gain (0x100138AE). So the stems add up to the mix, except for:
//!
//! - the rounding: the mix rounds to f32 after every source it adds, a stem rounds its own product,
//!   so the sum is a few f32 ulps off (below one int16 step);
//! - the denormal guard the master buses start with ([`super::GUARD`], 1e-6 times the master gain),
//!   which is in no stem;
//! - the compressor on the master (comp id 1), which works on the sum: the stems are taken before
//!   it.

use super::{BLOCK, Block, MAX_FRAMES};

/// The sources that have a stem.
pub const SOURCES: usize = 5;

/// A stem's source: mixer channel 0..3, or the delay's return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Tb303_1,
    Tb303_2,
    Tr808,
    Tr909,
    Delay,
}

impl Source {
    pub const ALL: [Source; SOURCES] = [
        Source::Tb303_1,
        Source::Tb303_2,
        Source::Tr808,
        Source::Tr909,
        Source::Delay,
    ];

    /// The mixer channel's index, 4 for the delay.
    pub fn index(self) -> usize {
        self as usize
    }
}

/// The delay's index in a [`StemBlock`].
pub(super) const DELAY: usize = Source::Delay as usize;

/// One block of every stem: `[source][0 left, 1 right]`. [`super::Frame::block`] writes it.
pub type StemBlock = [[Block; 2]; SOURCES];

pub(super) fn zero_block() -> Box<StemBlock> {
    Box::new([[[0.0; BLOCK]; 2]; SOURCES])
}

/// The stems of the last [`super::Engine::render`] call, frame for frame what that call
/// rendered: they go through the same leftover block as the mix.
pub struct Stems {
    /// `MAX_FRAMES` floats per source and side.
    out: Box<[f32]>,
    leftover: Box<StemBlock>,
    frames: usize,
}

impl Stems {
    pub(super) fn new() -> Self {
        Stems {
            out: vec![0.0; 2 * SOURCES * MAX_FRAMES].into_boxed_slice(),
            leftover: zero_block(),
            frames: 0,
        }
    }

    /// The frames the last render call rendered; 0 after a call that ended in a
    /// [`super::Fault`], which has no stems.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// One side (0 left, 1 right) of a source's stem: [`Stems::frames`] floats, at the scale of
    /// the mix before its int16 conversion ([`super::to_i16`]).
    pub fn get(&self, source: Source, side: usize) -> &[f32] {
        let at = (2 * source.index() + side) * MAX_FRAMES;
        &self.out[at..at + self.frames]
    }

    /// Every source's side: its output of the call and its leftover block.
    fn sides(&mut self) -> impl Iterator<Item = (&mut [f32; MAX_FRAMES], &mut Block)> {
        let (out, _) = self.out.as_chunks_mut::<MAX_FRAMES>();
        out.iter_mut().zip(self.leftover.iter_mut().flatten())
    }

    /// The start of a render call: nothing rendered yet.
    pub(super) fn begin(&mut self) {
        self.frames = 0;
    }

    /// Samples `lp..64` of the leftover block first.
    pub(super) fn head(&mut self, lp: usize) {
        for (o, lo) in self.sides() {
            o[..BLOCK - lp].copy_from_slice(&lo[lp..]);
        }
    }

    /// A whole block at frame `pos` of the call.
    pub(super) fn whole(&mut self, pos: usize, block: &StemBlock) {
        for ((o, _), b) in self.sides().zip(block.iter().flatten()) {
            o[pos..pos + BLOCK].copy_from_slice(b);
        }
    }

    /// The partial block, rendered whole into the leftover block.
    pub(super) fn partial(&mut self, block: &StemBlock) {
        *self.leftover = *block;
    }

    /// The first `k` samples of the leftover block at frame `pos`.
    pub(super) fn tail(&mut self, pos: usize, k: usize) {
        for (o, lo) in self.sides() {
            o[pos..pos + k].copy_from_slice(&lo[..k]);
        }
    }

    /// The end of a render call of `n` frames.
    pub(super) fn end(&mut self, n: usize) {
        self.frames = n;
    }

    /// Msg 33 zeroes the mix's leftover block.
    pub(super) fn reset(&mut self) {
        *self.leftover = [[[0.0; BLOCK]; 2]; SOURCES];
    }
}
