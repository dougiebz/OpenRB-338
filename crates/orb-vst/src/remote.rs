//! The original's Remote MIDI Control ("Standard Mapping") on one MIDI channel, off by
//! default. The mapping itself is `orb_player::midi::StandardMapping` (taken from upstream
//! OpenRB-338, GPL-3.0-only); this file only feeds it the host's events and applies what it
//! decides to the engine.
//!
//! Knobs, switches, the mixer, the shuffle and the pattern selection are applied at the start
//! of the buffer, like a note: they do not move the host's parameters. The transport actions
//! (play, stop, record, locate) are dropped, because the host owns the transport. The view
//! actions (bank, focus, instrument, keyboard mode) have no panel here and are dropped too.
//! docs/standard_midi_mapping.md lists the messages.

use orb_engine::frame::Engine;
use orb_player::midi::{Action, Message, StandardMapping, Timed};

/// The channel the mapping listens to unless told otherwise: MIDI channel 16 (0-based 15).
pub const CHANNEL: u8 = 15;

pub struct Remote {
    map: StandardMapping,
    out: Vec<Timed<Action>>,
}

impl Default for Remote {
    fn default() -> Self {
        let mut map = StandardMapping::new();
        map.channel = CHANNEL;
        Self {
            map,
            // room for a buffer's events, so the audio thread does not allocate
            out: Vec::with_capacity(256),
        }
    }
}

impl Remote {
    /// Feeds one message and applies the actions it gives.
    pub fn apply(&mut self, engine: &mut Engine, m: Message) {
        self.out.clear();
        self.map.message(0, &m, &mut self.out);
        for a in &self.out {
            match a.event {
                Action::DeviceParam {
                    device,
                    ctrl,
                    value,
                } => engine.device_param(device, ctrl, value),
                Action::MixerGlobal { id, value } => engine.mixer_global(id, value),
                Action::MixerChannel {
                    channel,
                    field,
                    value,
                } => engine.mixer_channel(channel, field, value),
                Action::Shuffle(v) => engine.set_shuffle(v),
                Action::Play
                | Action::Stop
                | Action::Record(_)
                | Action::Locate(_)
                | Action::Bank { .. }
                | Action::Focus(_)
                | Action::Instrument { .. }
                | Action::KeyMode(_) => {}
            }
        }
    }
}

/// The host's note or controller as a complete MIDI message; `None` for anything else.
pub fn message(event: nih_plug::prelude::NoteEvent<()>) -> Option<Message> {
    use nih_plug::prelude::NoteEvent;
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 127.0).round() as u8;
    match event {
        NoteEvent::NoteOn {
            channel,
            note,
            velocity,
            ..
        } => Some(Message::NoteOn {
            channel,
            note,
            velocity: byte(velocity),
        }),
        NoteEvent::NoteOff {
            channel,
            note,
            velocity,
            ..
        } => Some(Message::NoteOff {
            channel,
            note,
            velocity: byte(velocity),
        }),
        NoteEvent::MidiCC {
            channel, cc, value, ..
        } => Some(Message::ControlChange {
            channel,
            controller: cc,
            value: byte(value),
        }),
        _ => None,
    }
}

/// The channel (0-based) of a host event, for the events that have one.
pub fn channel(event: &nih_plug::prelude::NoteEvent<()>) -> Option<u8> {
    use nih_plug::prelude::NoteEvent;
    match *event {
        NoteEvent::NoteOn { channel, .. }
        | NoteEvent::NoteOff { channel, .. }
        | NoteEvent::MidiCC { channel, .. } => Some(channel),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nih_plug::prelude::NoteEvent;

    #[test]
    fn a_controller_on_the_channel_becomes_a_message() {
        let e = NoteEvent::MidiCC {
            timing: 0,
            channel: CHANNEL,
            cc: 74,
            value: 1.0,
        };
        assert_eq!(channel(&e), Some(CHANNEL));
        assert_eq!(
            message(e),
            Some(Message::ControlChange {
                channel: CHANNEL,
                controller: 74,
                value: 127
            })
        );
    }

    #[test]
    fn a_note_keeps_its_channel_and_velocity() {
        let e = NoteEvent::NoteOn {
            timing: 0,
            voice_id: None,
            channel: 3,
            note: 40,
            velocity: 0.5,
        };
        assert_eq!(
            message(e),
            Some(Message::NoteOn {
                channel: 3,
                note: 40,
                velocity: 64
            })
        );
    }

    #[test]
    fn other_events_are_not_midi_messages() {
        let e = NoteEvent::MidiChannelPressure {
            timing: 0,
            channel: 0,
            pressure: 0.5,
        };
        assert_eq!(message(e), None);
        assert_eq!(channel(&e), None);
    }

    #[test]
    fn mapped_controllers_reach_the_engine_and_the_transport_does_not() {
        use orb_engine::frame::Devices;
        use orb_engine::tables::Tables;
        use std::sync::Arc;
        let mut engine = Engine::new(
            Arc::new(Tables::new()),
            Devices::implemented(Some(rbm::standard::builtin_samples())),
        );
        let mut remote = Remote::default();
        // every mapped controller on the channel is applied without a panic; the wrong channel
        // is ignored by the mapping
        for cc in 0..128u8 {
            for channel in [CHANNEL, 0] {
                remote.apply(
                    &mut engine,
                    Message::ControlChange {
                        channel,
                        controller: cc,
                        value: 100,
                    },
                );
            }
        }
        // the transport notes are dropped: the engine is not playing
        for note in 0..128u8 {
            remote.apply(
                &mut engine,
                Message::NoteOn {
                    channel: CHANNEL,
                    note,
                    velocity: 100,
                },
            );
        }
        assert!(!engine.playing());
    }
}
