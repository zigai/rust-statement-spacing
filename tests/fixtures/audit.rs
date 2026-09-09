#![allow(dead_code)]

use std::fs;
use std::hint::black_box;
use std::path::Path;

const PYTHON: &str = r#"import json
print(json.dumps({}))
"#;

const TYPESCRIPT: &str = r#"const value = {};
console.log(value);
"#;

const FIRST: u32 = 1;
const SECOND: u32 = 2;

fn parallel_accumulators(values: &[i32]) -> (i32, i32) {
    let mut attack = 0_i32;
    let mut health = 0_i32;
    for value in values {
        attack = attack.saturating_add(*value);
        health = health.saturating_add(*value);
    }
    (attack, health)
}

fn loop_state(value: &str) -> String {
    let mut result = String::new();
    let mut last_dash = false;
    for character in value.to_lowercase().chars() {
        result.push(character);
        last_dash = character == '-';
    }

    black_box(last_dash);

    result
}

fn nested_accumulator(clips: &[Vec<u32>], width: u32, height: u32) -> Vec<u32> {
    let x_origin = width / 2;
    let y_origin = height / 2;

    let mut masks = Vec::new();
    for clip in clips {
        for frame in clip {
            masks.push(frame + x_origin + y_origin);
        }
    }

    masks
}

struct Status(bool);

impl Status {
    fn is_success(&self) -> bool {
        self.0
    }
}

fn status_check(response: bool) -> Result<(), ()> {
    let response = Status(response);
    let status = &response;
    if !status.is_success() {
        return Err(());
    }
    return Ok(());
}

fn names_check(content: &[String]) {
    for line in content {
        let names: Vec<_> = line.split_whitespace().collect();
        if names.is_empty() {
            continue;
        }

        black_box(names);
    }
}

fn rejection(candidates: &[u64], width: u64, threshold: u64) {
    for candidate in candidates {
        let product = u128::from(*candidate) * u128::from(width);
        if (product as u64) < threshold {
            continue;
        }

        black_box(product);
    }
}

fn string_derivation(input: &str) -> usize {
    let t = input.trim();
    let hex = t.strip_prefix('#').unwrap_or(t);

    if hex.len() == 6 {
        black_box(hex);
    }

    return hex.len();
}

struct Entry {
    key: String,
    value: String,
}

fn parallel_fields(entry: &Entry) -> Option<usize> {
    let key = entry.key.trim();
    let value = entry.value.trim();
    if key.is_empty() && value.is_empty() {
        return None;
    }
    return Some(key.len() + value.len());
}

fn validate_target(value: &str) -> Result<(), ()> {
    black_box(value);

    Ok(())
}

fn validate_name(value: &str) -> Result<(), ()> {
    black_box(value);

    Ok(())
}

fn validators(target: &str, name: &str) -> Result<(), ()> {
    validate_target(target)?;
    validate_name(name)?;

    Ok(())
}

struct Sim {
    golden: bool,
    cards: Vec<u32>,
}

impl Sim {
    fn add_card(&mut self, side: u32) {
        self.cards.push(side);
    }
}

fn conditional_repeat(sim: &mut Sim, side: u32) {
    sim.add_card(side);
    if sim.golden {
        sim.add_card(side);
    }
}

struct Ui;

impl Ui {
    fn checkbox(&mut self, value: &mut bool, label: &str) -> &mut Self {
        black_box((value, label));

        self
    }

    fn changed(&self) -> bool {
        true
    }
}

fn ui_ladder(ui: &mut Ui, first: &mut bool, second: &mut bool) -> bool {
    let mut dirty = false;
    dirty |= ui
        .checkbox(
            first,
            "Stop simulation automatically when there is exactly one remaining player",
        )
        .changed();
    dirty |= ui
        .checkbox(
            second,
            "Despawn all eliminated entities immediately after the elimination round",
        )
        .changed();

    dirty
}

fn verify(bundle: &Path) -> Result<(), ()> {
    black_box(bundle);

    Err(())
}

#[test]
fn iterative_scenarios() {
    let bundle = Path::new("unused-fixture-path");
    fs::write(bundle.join("extra"), b"extra").expect("extra file");
    assert!(verify(bundle).is_err());

    fs::remove_file(bundle.join("extra")).expect("remove extra file");
    fs::write(bundle.join("tracks.jsonl"), b"duplicate").expect("duplicate inventory");
    assert!(verify(bundle).is_err());

    fs::remove_file(bundle.join("tracks.jsonl")).expect("cleanup");
}

#[test]
fn fixture_mutation_rounds() {
    let mut a = Vec::new();
    let mut b = Vec::new();
    a.push((
        "first fixture",
        "a multiline fixture value long enough to retain the setup layout after rustfmt",
    ));
    b.push((
        "second fixture",
        "another multiline fixture value long enough to retain the setup layout after rustfmt",
    ));
    let mut enabled = true;
    assert!(enabled && a.len() == b.len());

    enabled = false;
    assert!(!enabled && a.len() == b.len());

    enabled = true;
    b.clear();
    b.push(("replacement", "value"));
    assert!(enabled && a.len() == b.len());

    a.clear();
    assert!(a.is_empty());

    b.clear();

    assert!(a.is_empty());
}
