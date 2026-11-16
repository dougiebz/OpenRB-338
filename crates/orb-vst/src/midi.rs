//! OpenRB's own MIDI map for the instrument. This is not ReBirth's MIDI input:
//! that code (`sub_100261F0` and the exe's MIDI path) was not traced.
//!
//! Channels are 0-based, as the plugin API numbers them.
//!
//! - Channel 0 (MIDI 1): TB-303 #1 notes and controller CCs.
//! - Channel 1 (MIDI 2): TB-303 #2, the same CCs.
//! - Channel 2 (MIDI 3) and channel 9 (MIDI 10): TR-808 hits.
//! - Channel 3 (MIDI 4) and channel 10 (MIDI 11): TR-909 hits.
//!
//! A 303 note uses MIDI 36 as the pattern's low C (note byte 0). Velocity 100
//! or above sets the accent flag. A second note while one is held slides to
//! it. The 303 is monophonic: the newest held note sounds. Releasing it while
//! older notes are still held slides back to the newest of those; releasing
//! the last held note calls the 303's note-off. CC 120 and CC 123 release
//! every held note.
//!
//! CCs, on the 303 channels: 2 tune, 3 cutoff, 4 resonance, 5 env mod, 6 decay,
//! 8 waveform (the byte is stored as sent; the engine treats non-zero as
//! square), 77 accent (controller 7). CC 7 on every channel is the master
//! level, so it is not the 303's accent.

use orb_engine::frame::Engine;
use orb_engine::song::Device;
use orb_engine::tb303::{self, Tb303};
use orb_engine::tr808;
use orb_engine::tr909;

/// One note or controller the plugin applies before rendering a buffer.
pub enum Event {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8 },
    Cc { channel: u8, cc: u8, value: u8 },
}

/// The notes held on one 303, oldest first; the last one sounds.
#[derive(Default)]
struct Held {
    notes: [u8; Held::MAX],
    len: usize,
}

impl Held {
    /// More keys than this at once drop the oldest.
    const MAX: usize = 8;

    fn top(&self) -> Option<u8> {
        self.len.checked_sub(1).map(|i| self.notes[i])
    }

    /// Takes `note` out, wherever it is. Returns whether it was held.
    fn remove(&mut self, note: u8) -> bool {
        let Some(at) = self.notes[..self.len].iter().position(|&n| n == note) else {
            return false;
        };
        self.notes.copy_within(at + 1..self.len, at);
        self.len -= 1;
        true
    }

    fn push(&mut self, note: u8) {
        self.remove(note);
        if self.len == Self::MAX {
            self.notes.copy_within(1.., 0);
            self.len -= 1;
        }
        self.notes[self.len] = note;
        self.len += 1;
    }
}

/// Which 303 notes are held, so note-off and slide know the current one.
#[derive(Default)]
pub struct Keys {
    held: [Held; 2],
}

impl Keys {
    pub fn apply(&mut self, engine: &mut Engine, event: Event) {
        match event {
            Event::NoteOn {
                channel,
                note,
                velocity,
            } => self.note_on(engine, channel, note, velocity),
            Event::NoteOff { channel, note } => self.note_off(engine, channel, note),
            // CC 120 all sound off, CC 123 all notes off
            Event::Cc { cc: 120 | 123, .. } => self.all_off(engine),
            Event::Cc { channel, cc, value } => cc_to(engine, channel, cc, value),
        }
    }

    fn note_on(&mut self, engine: &mut Engine, channel: u8, note: u8, velocity: u8) {
        if let Some(i) = synth_index(channel) {
            let slide = self.held[i].len > 0;
            self.held[i].push(note);
            if let Some(d) = engine.frame.devices.channels[i].as_tb303() {
                d.live_note(note, velocity, slide);
            }
            return;
        }
        let column = if channel == 2 || channel == 9 {
            tr808_column(note)
        } else if channel == 3 || channel == 10 {
            tr909_column(note)
        } else {
            None
        };
        let Some(column) = column else {
            return;
        };
        if channel == 2 || channel == 9 {
            if let Some(d) = engine.frame.devices.channels[2].as_tr808() {
                d.hit(column, velocity);
            }
        } else if let Some(d) = engine.frame.devices.channels[3].as_tr909() {
            d.hit(column, velocity);
        }
    }

    fn note_off(&mut self, engine: &mut Engine, channel: u8, note: u8) {
        let Some(i) = synth_index(channel) else {
            return;
        };
        let sounding = self.held[i].top() == Some(note);
        if !self.held[i].remove(note) || !sounding {
            // not held, or an older key under the one that sounds
            return;
        }
        let Some(d) = engine.frame.devices.channels[i].as_tb303() else {
            return;
        };
        match self.held[i].top() {
            // back to the key still held under it, without a new accent
            Some(under) => d.live_note(under, 0, true),
            None => Tb303::note_off(d),
        }
    }

    /// Releases every held note (a reset, or the host's all-notes-off).
    pub fn all_off(&mut self, engine: &mut Engine) {
        for i in 0..2 {
            self.held[i] = Held::default();
            if let Some(d) = engine.frame.devices.channels[i].as_tb303() {
                Tb303::note_off(d);
            }
        }
    }
}

fn synth_index(channel: u8) -> Option<usize> {
    match channel {
        0 => Some(0),
        1 => Some(1),
        _ => None,
    }
}

fn cc_to(engine: &mut Engine, channel: u8, cc: u8, value: u8) {
    if cc == 7 {
        crate::params::write(engine, crate::params::Target::Master, value);
        return;
    }
    let Some(i) = synth_index(channel) else {
        return;
    };
    let ctrl = match cc {
        2 => tb303::TUNE,
        3 => tb303::CUTOFF,
        4 => tb303::RESONANCE,
        5 => tb303::ENVMOD,
        6 => tb303::DECAY,
        8 => tb303::WAVEFORM,
        77 => tb303::ACCENT,
        _ => return,
    };
    let device = if i == 0 {
        Device::Tb303_1
    } else {
        Device::Tb303_2
    };
    crate::params::write(
        engine,
        crate::params::Target::Ctrl {
            device,
            ctrl: ctrl as u8,
        },
        value,
    );
}

/// 808 column for a GM-ish note, or none.
pub fn tr808_column(note: u8) -> Option<usize> {
    Some(match note {
        35 | 36 => tr808::BD,
        38 | 40 => tr808::SD,
        37 => tr808::RS,
        39 => tr808::CP,
        41 | 43 => tr808::LT,
        45 | 47 => tr808::MT,
        48 | 50 => tr808::HT,
        42 | 44 => tr808::CH,
        46 => tr808::OH,
        49 | 51 | 52 | 55 | 57 => tr808::CY,
        56 => tr808::CB,
        _ => return None,
    })
}

/// 909 column for a GM-ish note, or none.
pub fn tr909_column(note: u8) -> Option<usize> {
    Some(match note {
        35 | 36 => tr909::BD,
        38 | 40 => tr909::SD,
        37 => tr909::RS,
        39 => tr909::CP,
        41 | 43 => tr909::LT,
        45 | 47 => tr909::MT,
        48 | 50 => tr909::HT,
        42 | 44 => tr909::CH,
        46 => tr909::OH,
        49 | 52 | 55 | 57 => tr909::CC,
        51 | 53 | 59 => tr909::RC,
        _ => return None,
    })
}

/// A 0..1 plugin value to a controller byte.
pub fn byte_from_unit(v: f32) -> u8 {
    if !v.is_finite() || v <= 0.0 {
        0
    } else if v >= 1.0 {
        127
    } else {
        (v * 127.0).round() as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drum_notes() {
        assert_eq!(tr808_column(36), Some(tr808::BD));
        assert_eq!(tr808_column(42), Some(tr808::CH));
        assert_eq!(tr808_column(60), None);
        assert_eq!(tr909_column(51), Some(tr909::RC));
        assert_eq!(tr909_column(49), Some(tr909::CC));
    }

    #[test]
    fn cc_cutoff_uses_the_same_byte_as_a_host_parameter() {
        use orb_engine::frame::{Devices, Engine};
        use orb_engine::tables::Tables;
        use std::sync::Arc;
        let mut engine = Engine::new(Arc::new(Tables::new()), Devices::implemented(None));
        Keys::default().apply(
            &mut engine,
            Event::Cc {
                channel: 0,
                cc: 3,
                value: 40,
            },
        );
        assert_eq!(engine.frame.devices.channels[0].live()[tb303::CUTOFF], 40);
    }

    fn synth_engine() -> Engine {
        use orb_engine::frame::Devices;
        use orb_engine::tables::Tables;
        use std::sync::Arc;
        Engine::new(Arc::new(Tables::new()), Devices::implemented(None))
    }

    fn synth(engine: &mut Engine, i: usize) -> &mut Tb303 {
        engine.frame.devices.channels[i].as_tb303().unwrap()
    }

    fn on(keys: &mut Keys, engine: &mut Engine, channel: u8, note: u8) {
        keys.apply(
            engine,
            Event::NoteOn {
                channel,
                note,
                velocity: 90,
            },
        );
    }

    fn off(keys: &mut Keys, engine: &mut Engine, channel: u8, note: u8) {
        keys.apply(engine, Event::NoteOff { channel, note });
    }

    /// Overlapping notes left the gate open for good: the second note carried the
    /// slide flag, and the 303's note-off returns early while it is set.
    #[test]
    fn no_note_hangs_after_overlapping_notes() {
        for order in [[48u8, 50], [50, 48]] {
            let mut engine = synth_engine();
            let mut keys = Keys::default();
            on(&mut keys, &mut engine, 0, 48);
            on(&mut keys, &mut engine, 0, 50);
            assert_eq!(synth(&mut engine, 0).gate, 1.0);
            // released in either order
            off(&mut keys, &mut engine, 0, order[0]);
            assert_eq!(synth(&mut engine, 0).gate, 1.0, "one key is still down");
            off(&mut keys, &mut engine, 0, order[1]);
            assert_eq!(synth(&mut engine, 0).gate, 0.0, "released {order:?}");
        }
    }

    #[test]
    fn an_overlapping_note_slides_and_a_separate_one_does_not() {
        let mut engine = synth_engine();
        let mut keys = Keys::default();
        on(&mut keys, &mut engine, 1, 48);
        let first = synth(&mut engine, 1).pitch;
        on(&mut keys, &mut engine, 1, 60);
        let d = synth(&mut engine, 1);
        // the pitch glides from the first note: only the target moved
        assert_eq!(d.pitch, first);
        assert_ne!(d.pitch_target, first);
        off(&mut keys, &mut engine, 1, 48);
        off(&mut keys, &mut engine, 1, 60);
        // a new note after everything was released starts at its own pitch
        on(&mut keys, &mut engine, 1, 60);
        let d = synth(&mut engine, 1);
        assert_eq!(d.pitch, d.pitch_target);
        // the other 303 was never touched
        assert_eq!(synth(&mut engine, 0).gate, 0.0);
    }

    #[test]
    fn releasing_the_top_key_returns_to_the_one_under_it() {
        let mut engine = synth_engine();
        let mut keys = Keys::default();
        on(&mut keys, &mut engine, 0, 48);
        let low = synth(&mut engine, 0).pitch_target;
        on(&mut keys, &mut engine, 0, 55);
        off(&mut keys, &mut engine, 0, 55);
        let d = synth(&mut engine, 0);
        assert_eq!((d.gate, d.pitch_target), (1.0, low));
        off(&mut keys, &mut engine, 0, 48);
        assert_eq!(synth(&mut engine, 0).gate, 0.0);
    }

    #[test]
    fn all_notes_off_releases_both_synths() {
        let mut engine = synth_engine();
        let mut keys = Keys::default();
        on(&mut keys, &mut engine, 0, 48);
        on(&mut keys, &mut engine, 0, 52);
        on(&mut keys, &mut engine, 1, 40);
        keys.apply(
            &mut engine,
            Event::Cc {
                channel: 0,
                cc: 123,
                value: 0,
            },
        );
        assert_eq!(synth(&mut engine, 0).gate, 0.0);
        assert_eq!(synth(&mut engine, 1).gate, 0.0);
        // a late note-off of a key that is no longer held does nothing
        off(&mut keys, &mut engine, 0, 52);
        assert_eq!(synth(&mut engine, 0).gate, 0.0);
    }

    #[test]
    fn unit_to_byte() {
        assert_eq!(byte_from_unit(0.0), 0);
        assert_eq!(byte_from_unit(1.0), 127);
        assert_eq!(byte_from_unit(0.5), 64);
    }
}
