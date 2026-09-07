#![allow(dead_code)]

use std::hint::black_box;
use validate_non_empty as validate_again;

fn dependent_setup(values: &[f32]) {
    let sum: f32 = values.iter().sum();
    let diff = (sum - 1.0).abs();

    if diff > 0.001 {
        black_box(diff);
    }
}

fn paired_setup(window: (u32, u32)) {
    let width = window.0;
    let height = window.1;

    if width == 0 || height == 0 {
        return;
    }
}

fn parallel_setup(escapes: &[u8], successes: &[u8], failures: &[u8], eliminations: &[u8]) {
    let escape_count = escapes.len();
    let success_count = successes.len();
    let failure_count = failures.len();
    let elimination_count = eliminations.len();

    if escape_count + success_count + failure_count + elimination_count > 0 {
        black_box(elimination_count);
    }
}

fn mutation_and_counter(actions: &mut Vec<u32>) -> usize {
    let mut actions_added = 0;

    actions.push(7);
    actions_added += 1;

    actions_added
}

struct Inputs {
    initial: String,
    increment: String,
    origin_x: String,
    origin_y: String,
    enabled: bool,
}

fn clear_inputs(inputs: &mut Inputs) {
    inputs.initial.clear();
    inputs.increment.clear();
    inputs.origin_x.clear();
    inputs.origin_y.clear();
    inputs.enabled = false;
}

fn disable_inputs(inputs: &mut Inputs) {
    inputs.enabled = false;
    inputs.initial.clear();
}

fn cleanup(
    state: &mut Vec<u32>,
    resets: &mut std::vec::Drain<'_, u32>,
    updates: &mut std::vec::Drain<'_, u32>,
) {
    state.clear();
    for _ in resets {}
    for _ in updates {}
    return;
}

struct Ui {
    rows: usize,
    labels: Vec<&'static str>,
    spacing: f32,
}

impl Ui {
    fn label(&mut self, text: &'static str) {
        self.labels.push(text);
    }

    fn add_space(&mut self, amount: f32) {
        self.spacing += amount;
    }

    fn changed(&mut self) -> bool {
        self.rows > 0
    }

    fn end_row(&mut self) {
        self.rows += 1;
    }

    fn edit(&mut self, text: &mut String) -> bool {
        text.push('x');
        self.changed()
    }

    fn with(&mut self, action: impl FnOnce(&mut Self)) {
        action(self);
    }
}

fn ui_row(ui: &mut Ui) -> bool {
    let mut changed = false;

    if ui.changed() {
        changed = true;
    }
    ui.end_row();

    changed
}

fn ui_changed(ui: &mut Ui, id: String) -> bool {
    black_box(id);

    ui.changed()
}

fn ui_free_call_row(ui: &mut Ui) -> bool {
    let mut changed = false;

    if ui_changed(ui, format!("row-{}", 1)) {
        changed = true;
    }
    ui.end_row();

    changed
}

fn ui_heading(ui: &mut Ui) {
    ui.add_space(6.0);
    ui.label("Border");

    if ui.changed() {
        black_box(ui);
    }
}

fn find_segment(values: &[u32]) -> bool {
    let mut found = false;

    for value in values {
        if *value == 7 {
            black_box(value);

            found = true;
            break;
        }
    }

    found
}

fn validate_non_empty(field: &str, value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err(field.to_owned());
    }

    Ok(())
}

fn validations(user_data: &str, meta_data: &str) -> Result<(), String> {
    validate_non_empty("user_data", user_data)?;
    validate_again("meta_data", meta_data)?;

    Ok(())
}

fn backups(
    transaction: &mut Vec<String>,
    user_data: String,
    meta_data: String,
    network: Option<String>,
) {
    transaction.push(user_data);
    transaction.push(meta_data);

    if let Some(network) = network {
        transaction.push(network);
    }
}

fn alpha(value: u32) {
    black_box(value);
}

fn beta(value: u32) {
    black_box(value);
}

fn identity(value: u32) -> u32 {
    value
}

fn unrelated_calls(left: u32, right: u32) {
    alpha(left);

    beta(right);
}

fn nested_calls(left: u32, right: u32) {
    alpha(identity(left));

    beta(identity(right));
}

fn pure_read_and_update(values: &Vec<u32>, mut counter: usize) -> usize {
    values.len();

    counter += 1;

    counter
}

fn separate_controls(left: bool, right: bool) {
    if left {
        alpha(1);
    }

    if right {
        beta(2);
    }
}

fn buffered_row(ui: &mut Ui, source: &str) -> bool {
    let mut changed = false;

    ui.label("Id");
    let mut text = source.to_owned();
    if ui.edit(&mut text) {
        changed = true;
    }
    ui.end_row();

    changed
}

fn flagged_row(ui: &mut Ui) -> bool {
    let mut changed = false;

    ui.label("Trigger");
    let mut response_changed = false;
    ui.with(|ui| response_changed = ui.changed());
    if response_changed {
        changed = true;
    }
    ui.end_row();

    changed
}

fn nested_counter(values: &[u32]) -> usize {
    let mut count = 0;
    for value in values {
        if *value > 0 {
            count += 1;
        }
    }

    count
}

fn direct_return(value: f32) -> f32 {
    if value < 0.0 {
        return 0.0;
    }

    let scaled = value * 2.0;
    return scaled + 1.0;
}

fn plain_cleanup(first: &mut Vec<u32>, second: &mut Vec<u32>) {
    first.clear();
    second.clear();
    return;
}

fn sequential_loops(first: &[u32], second: &[u32], third: &[u32]) {
    for value in first {
        black_box(value);
    }

    for value in second {
        black_box(value);
    }

    for value in third {
        black_box(value);
    }
}

fn labeled_final_loop(first: &[u32], second: &[u32]) {
    for value in first {
        black_box(value);
    }

    'scan: for value in second {
        if *value > 0 {
            break 'scan;
        }
    }
}

fn final_while(first: &[u32], mut remaining: usize) {
    for value in first {
        black_box(value);
    }

    while remaining > 0 {
        remaining -= 1;
    }
}

fn loop_value(value: u32) -> u32 {
    loop {
        break value;
    }
}

macro_rules! generated_loop {
    ($values:expr) => {
        for value in $values {
            black_box(value);
        }
    };
}

fn macro_final_loop(values: &[u32]) {
    for value in values {
        black_box(value);
    }
    generated_loop!(values);
}

fn final_empty_drains(first: &[u32], second: &[u32]) {
    for _ in first {}
    for _ in second {}
}

fn terminal_widget_block(ui: &mut Ui, source: &str) {
    ui.with(|ui| {
        ui.label("Text");
        let mut text = source.to_owned();
        if ui.edit(&mut text) {
            black_box(|| text.len());
        }
    });
}
