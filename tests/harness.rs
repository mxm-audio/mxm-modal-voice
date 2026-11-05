//! The reference: every case in `cases/`, rendered by a fresh part and compared, by the SHA-256
//! of its outputs, with what Elements' C++ renders (`digests.txt`): its two float outputs, every
//! bit, and the module's 16-bit frames made from them. The case format and its semantics are
//! `cases/README.md`'s.
//!
//! Debug aids, by environment variable:
//!
//! - `MXM_MODAL_VOICE_RAW_DIR=<dir>`: write each rendered case's outputs to `<dir>/<case>.f32`
//!   and `<dir>/<case>.raw`, the formats of the C++ harness's renders.
//! - `MXM_MODAL_VOICE_REFERENCE_DIR=<dir>`: the C++ harness's renders, `<dir>/<case>.f32`; on a
//!   mismatch the first differing block and sample are printed from it.
//! - `MXM_MODAL_VOICE_CASE=<name>`: render only that case (`m0-strike`).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use mxm_modal_voice::dsp::K_MAX_BLOCK_SIZE;
use mxm_modal_voice::stmlib::dsp::dsp::{cast_i32, soft_convert};
use mxm_modal_voice::{Part, Patch, PerformanceState};
use sha2::{Digest, Sha256};

/// `kAudioChunkSize`: the firmware's block, and the harness's.
const BLOCK: usize = 16;

fn cases_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("cases")
}

// ---------------------------------------------------------------------------------------------
// Flush-to-zero

/// Sets MXCSR's flush-to-zero bit (15), not denormals-are-zero (6), as the C++ harness's
/// `_MM_SET_FLUSH_ZERO_MODE(_MM_FLUSH_ZERO_ON)` does. MXCSR is per thread, and each test runs on
/// its own thread, so every render sets it first.
#[cfg(target_arch = "x86_64")]
fn set_flush_to_zero() {
    let mut csr: u32 = 0;
    // SAFETY: `stmxcsr` stores MXCSR to the u32 the pointer names, and `ldmxcsr` loads it back
    // with one more bit: flush-to-zero changes only how results that would be denormal round,
    // which is what the reference was rendered with. Nothing else in the process is affected
    // beyond this thread's floating point.
    unsafe {
        std::arch::asm!("stmxcsr [{}]", in(reg) &mut csr, options(nostack, preserves_flags));
        csr |= 1 << 15;
        std::arch::asm!("ldmxcsr [{}]", in(reg) &csr, options(nostack, preserves_flags));
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn set_flush_to_zero() {}

// ---------------------------------------------------------------------------------------------
// The case format

#[derive(Clone, Copy, Debug)]
enum Field {
    Patch(fn(&mut Patch) -> &mut f32),
    Perf(fn(&mut PerformanceState) -> &mut f32),
    Gate,
    Model,
    EasterEgg,
}

fn field(name: &str) -> Option<Field> {
    use Field::*;
    Some(match name {
        "patch.exciter_envelope_shape" => Patch(|p| &mut p.exciter_envelope_shape),
        "patch.exciter_bow_level" => Patch(|p| &mut p.exciter_bow_level),
        "patch.exciter_bow_timbre" => Patch(|p| &mut p.exciter_bow_timbre),
        "patch.exciter_blow_level" => Patch(|p| &mut p.exciter_blow_level),
        "patch.exciter_blow_meta" => Patch(|p| &mut p.exciter_blow_meta),
        "patch.exciter_blow_timbre" => Patch(|p| &mut p.exciter_blow_timbre),
        "patch.exciter_strike_level" => Patch(|p| &mut p.exciter_strike_level),
        "patch.exciter_strike_meta" => Patch(|p| &mut p.exciter_strike_meta),
        "patch.exciter_strike_timbre" => Patch(|p| &mut p.exciter_strike_timbre),
        "patch.exciter_signature" => Patch(|p| &mut p.exciter_signature),
        "patch.resonator_geometry" => Patch(|p| &mut p.resonator_geometry),
        "patch.resonator_brightness" => Patch(|p| &mut p.resonator_brightness),
        "patch.resonator_damping" => Patch(|p| &mut p.resonator_damping),
        "patch.resonator_position" => Patch(|p| &mut p.resonator_position),
        "patch.resonator_modulation_frequency" => Patch(|p| &mut p.resonator_modulation_frequency),
        "patch.resonator_modulation_offset" => Patch(|p| &mut p.resonator_modulation_offset),
        "patch.reverb_diffusion" => Patch(|p| &mut p.reverb_diffusion),
        "patch.reverb_lp" => Patch(|p| &mut p.reverb_lp),
        "patch.space" => Patch(|p| &mut p.space),
        "patch.modulation_frequency" => Patch(|p| &mut p.modulation_frequency),
        "perf.gate" => Gate,
        "perf.note" => Perf(|s| &mut s.note),
        "perf.modulation" => Perf(|s| &mut s.modulation),
        "perf.strength" => Perf(|s| &mut s.strength),
        "part.model" => Model,
        "part.easter_egg" => EasterEgg,
        _ => return None,
    })
}

/// The harness's `assign`: a float as it is, a bool true unless the value is 0, the model
/// through `static_cast<int>` (as MSVC compiles it) into `set_resonator_model`.
fn assign(field: Field, value: f32, part: &mut Part, perf: &mut PerformanceState) {
    match field {
        Field::Patch(get) => *get(part.mutable_patch()) = value,
        Field::Perf(get) => *get(perf) = value,
        Field::Gate => perf.gate = value != 0.0,
        Field::Model => part.set_resonator_model(cast_i32(value) as u8),
        Field::EasterEgg => part.set_easter_egg(value != 0.0),
    }
}

struct Action {
    b0: i32,
    b1: i32,
    field: Field,
    v0: f32,
    v1: f32,
}

#[derive(Clone, Copy)]
enum Kind {
    Off,
    Noise,
    Pulse,
}

#[derive(Clone, Copy)]
struct Input {
    block: i32,
    which: usize,
    kind: Kind,
    amp: f32,
    period: i32,
}

/// A case parsed as the harness parses it: `set`, `seed` and the setters act at once, after
/// `Part::init`, in the file's order.
struct Case {
    blocks: usize,
    part: Box<Part>,
    perf: PerformanceState,
    actions: Vec<Action>,
    inputs: Vec<Input>,
}

/// A value as `sscanf`'s `%f` reads it into a `float`: the longest prefix that is a decimal
/// number, rounded once to the nearest float (as the UCRT and Rust both do).
fn scan_f32(word: &str) -> Option<f32> {
    let bytes = word.as_bytes();
    let mut end = 0;
    if matches!(bytes.first(), Some(b'+' | b'-')) {
        end += 1;
    }
    let digits_start = end;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
    }
    if end == digits_start || (end == digits_start + 1 && bytes[digits_start] == b'.') {
        return None;
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let mut exponent = end + 1;
        if matches!(bytes.get(exponent), Some(b'+' | b'-')) {
            exponent += 1;
        }
        if bytes.get(exponent).is_some_and(u8::is_ascii_digit) {
            while bytes.get(exponent).is_some_and(u8::is_ascii_digit) {
                exponent += 1;
            }
            end = exponent;
        }
    }
    word[..end].parse().ok()
}

/// A value as `%d` reads it.
fn scan_i32(word: &str) -> Option<i32> {
    let bytes = word.as_bytes();
    let mut end = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let digits_start = end;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if end == digits_start {
        return None;
    }
    word[..end].parse().ok()
}

fn parse_case(name: &str, text: &str) -> Case {
    let mut part = Part::new();
    part.init();
    let mut case = Case {
        blocks: 0,
        part,
        perf: PerformanceState::default(),
        actions: Vec::new(),
        inputs: Vec::new(),
    };
    let mut blocks: i32 = -1;
    for (number, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("");
        let words: Vec<&str> = line.split_whitespace().collect();
        let Some(&directive) = words.first() else {
            continue;
        };
        macro_rules! bad {
            () => {
                panic!("{name}:{}: bad line {line:?}", number + 1)
            };
        }
        let field_of =
            |word: &str| field(word).unwrap_or_else(|| panic!("{name}: unknown field {word}"));
        match directive {
            "blocks" => {
                blocks = words
                    .get(1)
                    .and_then(|w| scan_i32(w))
                    .unwrap_or_else(|| bad!())
            }
            "set" => {
                let (Some(f), Some(v)) = (words.get(1), words.get(2).and_then(|w| scan_f32(w)))
                else {
                    bad!()
                };
                assign(field_of(f), v, &mut case.part, &mut case.perf);
            }
            "at" => {
                let (Some(b0), Some(f), Some(v)) = (
                    words.get(1).and_then(|w| scan_i32(w)),
                    words.get(2),
                    words.get(3).and_then(|w| scan_f32(w)),
                ) else {
                    bad!()
                };
                case.actions.push(Action {
                    b0,
                    b1: b0,
                    field: field_of(f),
                    v0: v,
                    v1: v,
                });
            }
            "ramp" => {
                let (Some(b0), Some(b1), Some(f), Some(v0), Some(v1)) = (
                    words.get(1).and_then(|w| scan_i32(w)),
                    words.get(2).and_then(|w| scan_i32(w)),
                    words.get(3),
                    words.get(4).and_then(|w| scan_f32(w)),
                    words.get(5).and_then(|w| scan_f32(w)),
                ) else {
                    bad!()
                };
                if b1 <= b0 {
                    bad!();
                }
                case.actions.push(Action {
                    b0,
                    b1,
                    field: field_of(f),
                    v0,
                    v1,
                });
            }
            "seed" => {
                let words: Option<Vec<u32>> =
                    words[1..].iter().map(|w| w.parse::<u32>().ok()).collect();
                match words {
                    Some(words) if words.len() == 3 => case.part.seed(&words),
                    _ => bad!(),
                }
            }
            "input" => {
                let (Some(block), Some(which), Some(kind), Some(amp), Some(period)) = (
                    words.get(1).and_then(|w| scan_i32(w)),
                    words.get(2),
                    words.get(3),
                    words.get(4).and_then(|w| scan_f32(w)),
                    words.get(5).and_then(|w| scan_i32(w)),
                ) else {
                    bad!()
                };
                let which = match *which {
                    "blow" => 0,
                    "strike" => 1,
                    _ => bad!(),
                };
                let kind = match *kind {
                    "off" => Kind::Off,
                    "noise" => Kind::Noise,
                    "pulse" if period > 0 => Kind::Pulse,
                    _ => bad!(),
                };
                case.inputs.push(Input {
                    block,
                    which,
                    kind,
                    amp,
                    period,
                });
            }
            _ => bad!(),
        }
    }
    assert!(blocks > 0, "{name}: no blocks");
    case.blocks = blocks as usize;
    case
}

// ---------------------------------------------------------------------------------------------
// The renderer

/// The case's part renders it, block by block: its floats, interleaved main, aux, and the
/// module's frames, interleaved left `SoftConvert(aux)`, right `SoftConvert(main)`; both
/// little-endian.
fn render(mut case: Case) -> (Vec<u8>, Vec<u8>) {
    set_flush_to_zero();
    let part = &mut case.part;
    let mut perf = case.perf;
    let mut floats = Vec::with_capacity(case.blocks * BLOCK * 8);
    let mut frames = Vec::with_capacity(case.blocks * BLOCK * 4);
    let mut current = [
        Input {
            block: 0,
            which: 0,
            kind: Kind::Off,
            amp: 0.0,
            period: 1,
        },
        Input {
            block: 0,
            which: 1,
            kind: Kind::Off,
            amp: 0.0,
            period: 1,
        },
    ];
    let mut lcg: [u32; 2] = [1, 2]; // each input's noise
    let mut inputs = [[0.0f32; BLOCK]; 2];
    let mut main = [0.0f32; K_MAX_BLOCK_SIZE];
    let mut aux = [0.0f32; K_MAX_BLOCK_SIZE];
    for block in 0..case.blocks as i32 {
        for a in &case.actions {
            if block < a.b0 || block > a.b1 {
                continue;
            }
            let t = if a.b1 == a.b0 {
                0.0
            } else {
                (block - a.b0) as f32 / (a.b1 - a.b0) as f32
            };
            assign(a.field, a.v0 + (a.v1 - a.v0) * t, part, &mut perf);
        }
        for i in &case.inputs {
            if i.block == block {
                current[i.which] = *i;
            }
        }
        for w in 0..2 {
            for (s, value) in inputs[w].iter_mut().enumerate() {
                let sample = block as usize * BLOCK + s;
                *value = match current[w].kind {
                    Kind::Off => 0.0,
                    Kind::Noise => {
                        lcg[w] = lcg[w].wrapping_mul(1664525).wrapping_add(1013904223);
                        f32::from((lcg[w] >> 16) as i16) / 32768.0 * current[w].amp
                    }
                    Kind::Pulse => {
                        if sample.is_multiple_of(current[w].period as usize) {
                            current[w].amp
                        } else {
                            0.0
                        }
                    }
                };
            }
        }
        part.process(&perf, &inputs[0], &inputs[1], &mut main, &mut aux, BLOCK);
        for s in 0..BLOCK {
            floats.extend_from_slice(&main[s].to_le_bytes());
            floats.extend_from_slice(&aux[s].to_le_bytes());
            frames.extend_from_slice(&soft_convert(aux[s]).to_le_bytes());
            frames.extend_from_slice(&soft_convert(main[s]).to_le_bytes());
        }
    }
    (floats, frames)
}

/// `digests.txt`: the C++'s digest of each case's `.f32` and `.raw`.
fn digests() -> BTreeMap<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("digests.txt");
    let text = std::fs::read_to_string(&path).expect("tests/digests.txt");
    text.lines()
        .filter_map(|line| {
            let (digest, file) = line.split_once(" *")?;
            Some((file.trim().to_string(), digest.trim().to_lowercase()))
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Where our floats first differ from the C++'s, block and sample, when its render is at hand.
fn first_difference(case: &str, ours: &[u8]) -> String {
    let Some(dir) = std::env::var_os("MXM_MODAL_VOICE_REFERENCE_DIR") else {
        return "set MXM_MODAL_VOICE_REFERENCE_DIR to the C++ renders to see the first difference"
            .to_string();
    };
    let path = Path::new(&dir).join(format!("{case}.f32"));
    let Ok(theirs) = std::fs::read(&path) else {
        return format!("no reference render at {}", path.display());
    };
    let sample = |bytes: &[u8], i: usize| {
        f32::from_le_bytes([
            bytes[4 * i],
            bytes[4 * i + 1],
            bytes[4 * i + 2],
            bytes[4 * i + 3],
        ])
    };
    let samples = ours.len().min(theirs.len()) / 4;
    match (0..samples).find(|&i| sample(ours, i).to_bits() != sample(&theirs, i).to_bits()) {
        None if ours.len() == theirs.len() => "identical to the reference render".to_string(),
        None => format!(
            "{} bytes against the reference's {}",
            ours.len(),
            theirs.len()
        ),
        Some(i) => {
            let frame = i / 2;
            let channel = if i % 2 == 0 { "main" } else { "aux" };
            format!(
                "first difference at block {}, sample {} ({channel}): ours {:e}, the C++ {:e}",
                frame / BLOCK,
                frame % BLOCK,
                sample(ours, i),
                sample(&theirs, i)
            )
        }
    }
}

#[test]
fn reference_cases() {
    let digests = digests();
    let only = std::env::var("MXM_MODAL_VOICE_CASE").ok();
    let raw_dir = std::env::var_os("MXM_MODAL_VOICE_RAW_DIR").map(PathBuf::from);
    let mut names: Vec<String> = std::fs::read_dir(cases_dir())
        .expect("tests/cases")
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().into_string().ok()?;
            Some(name.strip_suffix(".case")?.to_string())
        })
        .collect();
    names.sort();

    let mut ran = 0;
    let mut failures = Vec::new();
    for name in &names {
        if only.as_ref().is_some_and(|only| only != name) {
            continue;
        }
        let text = std::fs::read_to_string(cases_dir().join(format!("{name}.case"))).expect("case");
        let (floats, frames) = render(parse_case(name, &text));
        if let Some(dir) = &raw_dir {
            std::fs::create_dir_all(dir).expect("MXM_MODAL_VOICE_RAW_DIR");
            std::fs::write(dir.join(format!("{name}.f32")), &floats).expect("write f32");
            std::fs::write(dir.join(format!("{name}.raw")), &frames).expect("write raw");
        }
        let expect = |ext: &str| {
            digests
                .get(&format!("{name}.{ext}"))
                .unwrap_or_else(|| panic!("{name}.{ext}: no digest"))
        };
        ran += 1;
        let floats_match = &hex(&Sha256::digest(&floats)) == expect("f32");
        let frames_match = &hex(&Sha256::digest(&frames)) == expect("raw");
        if floats_match && frames_match {
            println!("{name}: bit for bit");
        } else {
            let at = first_difference(name, &floats);
            println!("{name}: MISMATCH (floats {floats_match}, frames {frames_match}; {at})");
            failures.push(format!("{name}: {at}"));
        }
    }
    assert!(ran > 0, "no case ran");
    assert!(
        failures.is_empty(),
        "{} of {ran} cases differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn the_case_values_round_once() {
    // `%f` into a float: whether the UCRT rounds the decimal to float once, or to double and
    // then to float, gives the same float for every value in the cases (so the parse above is
    // the C++'s either way).
    for entry in std::fs::read_dir(cases_dir()).expect("tests/cases") {
        let path = entry.expect("entry").path();
        if path.extension().is_none_or(|e| e != "case") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("case");
        for word in text
            .lines()
            .flat_map(|l| l.split('#').next().unwrap_or("").split_whitespace())
        {
            if let Some(once) = scan_f32(word) {
                let twice = word.parse::<f64>().map(|d| d as f32).unwrap_or(once);
                assert_eq!(
                    once.to_bits(),
                    twice.to_bits(),
                    "{}: {word}",
                    path.display()
                );
            }
        }
    }
}
