//! The stems ([`orb_engine::frame::stems`]) against the reference corpus: with the stems on,
//! the songs still render equal to the original's references, and the stems add up to the mix.

mod common;

use std::sync::Arc;

use common::{manifest, samples, sha256, song};
use orb_engine::frame::export::{self, ExportRange};
use orb_engine::frame::stems::Source;
use orb_engine::frame::{Activation, Devices, Engine, to_i16};
use orb_engine::tables::Tables;

/// Songs with every device, the inserts, the delay and the mixer's sweeps; a release build
/// takes every diagnostic song instead.
const SONGS: [&str; 12] = [
    "303_two_voices",
    "comp_ch_808",
    "comp_master_a100_t10",
    "delay_303",
    "delay_pan0",
    "dist_drums",
    "drums_kit",
    "full_mix",
    "mixer_delay_send_sweep",
    "mixer_pan_sweep_909",
    "pcf_bp_wave127",
    "pcf_on_909",
];

fn names() -> Vec<String> {
    if cfg!(debug_assertions) {
        return SONGS.iter().map(|s| s.to_string()).collect();
    }
    let m = manifest();
    m["diag"].as_object().unwrap().keys().cloned().collect()
}

/// With the stems on, the export equals the original's reference (the WAV's sha256).
#[test]
fn the_references_match_with_the_stems_on() {
    let m = manifest();
    let tables = Arc::new(Tables::new());
    let set = samples();
    let mut bad = Vec::new();
    for name in names() {
        let e = &m["diag"][&name];
        let idle = e["idle_blocks"].as_u64().unwrap() as u32;
        let mut eng = Engine::new(tables.clone(), Devices::implemented(Some(set.clone())));
        eng.set_stems(true);
        let got = export::export_song(&mut eng, &song(e), idle).unwrap();
        if sha256(&export::wav_bytes(&got)) != e["wav_sha256"].as_str().unwrap() {
            bad.push(name);
        }
    }
    assert!(bad.is_empty(), "differ from their references: {bad:?}");
}

/// The export's chunks of the diag song `name` with the stems on: the largest difference, in
/// int16 steps, between the mix and the stems' sum, and which sources sounded.
fn sum_against_mix(name: &str) -> (i32, [bool; 5]) {
    let m = manifest();
    let e = &m["diag"][name];
    let s = song(e);
    let devices = Devices::implemented(Some(samples()));
    let mut eng = Engine::new(Arc::new(Tables::new()), devices);
    eng.set_stems(true);
    eng.load(&s).unwrap();
    eng.activate(Activation::ReplacesCurrent {
        current_playing: false,
    });
    export::render_idle(&mut eng, e["idle_blocks"].as_u64().unwrap() as u32);
    let range = ExportRange::from(&s.globals);
    eng.stop();
    eng.locate(range.loop_start);
    eng.play();
    let mut buf = vec![0i16; 2 * export::CHUNK_FRAMES as usize];
    let mut worst = 0;
    let mut sounded = [false; 5];
    for n in export::chunk_sizes(range) {
        let n = n.max(0) as usize;
        eng.render(&mut buf, n);
        let st = eng.stems().unwrap();
        assert_eq!(st.frames(), n);
        for side in 0..2 {
            let parts = Source::ALL.map(|src| st.get(src, side));
            for i in 0..n {
                let sum: f64 = parts.iter().map(|p| p[i] as f64).sum();
                let d = to_i16(sum as f32) as i32 - buf[2 * i + side] as i32;
                worst = worst.max(d.abs());
            }
            for (k, p) in parts.iter().enumerate() {
                // above an idle delay's 1e-6
                sounded[k] |= p.iter().any(|x| x.abs() > 1e-3);
            }
        }
    }
    (worst, sounded)
}

/// Without the compressor on the master the stems add up to the mix: at most one int16 step
/// apart (the mix rounds after every source, and its 1e-6 guard is in no stem).
#[test]
fn the_stems_add_up_to_the_mix() {
    for (name, want) in [
        ("303_two_voices", [true, true, false, false, false]),
        ("drums_kit", [false, false, true, true, false]),
        ("delay_303", [true, false, false, false, true]),
        ("mixer_pan_sweep_909", [false, false, false, true, false]),
    ] {
        let (worst, sounded) = sum_against_mix(name);
        assert!(worst <= 1, "{name}: {worst} int16 steps");
        assert_eq!(sounded, want, "{name}");
    }
}

/// With the compressor on the master the stems are what goes into it, so their sum is not the
/// mix.
#[test]
fn the_master_compressor_is_after_the_stems() {
    let (worst, _) = sum_against_mix("comp_master_a127_t0");
    assert!(worst > 1, "{worst}");
}
