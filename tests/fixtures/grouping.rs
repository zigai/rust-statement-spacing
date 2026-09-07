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

fn completion_returns(values: &mut Vec<u32>) -> Result<(), &'static str> {
    values.push(1);
    values.push(2);
    return Ok(());
}

fn completion_closure(values: &mut Vec<u32>) {
    let _ = || -> Result<(), &'static str> {
        values.push(1);
        values.push(2);
        Ok(())
    };
}

fn guard_return(output: std::process::Output) -> Option<String> {
    if !output.status.success() {
        return None;
    }
    return String::from_utf8(output.stdout).ok();
}

fn prefix_lengths(needle: &[u8], prefix_lengths: &mut [usize], mut matched: usize) {
    for index in 1..needle.len() {
        let byte = needle[index];
        while matched > 0 && needle[matched] != byte {
            matched = prefix_lengths[matched - 1];
        }
        if needle[matched] == byte {
            matched += 1;
        }
        prefix_lengths[index] = matched;
    }
}

fn polling(pid_path: &std::path::Path, deadline: std::time::Instant) -> std::io::Result<String> {
    while !pid_path.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let recorded_pid = std::fs::read_to_string(pid_path)?;
    return Ok(recorded_pid);
}

unsafe extern "C" {
    fn fill_value(value: *mut std::ffi::c_void) -> i32;
}

fn check_status(status: i32) -> Result<(), i32> {
    if status != 0 {
        return Err(status);
    }
    Ok(())
}

fn ffi_output() -> Result<u32, i32> {
    let mut value = 0_u32;

    // SAFETY: value is live and writable for the duration of the call; the
    // fixture's external contract accepts an aligned u32 output pointer.
    check_status(unsafe { fill_value((&raw mut value).cast::<std::ffi::c_void>()) })?;
    return Ok(value);
}

fn borrowed_output() -> u32 {
    let mut value = 0;

    std::mem::swap(&mut value, &mut 3);
    return value;
}

fn completion_none(values: &mut Vec<u32>) -> Option<u32> {
    values.push(1);
    values.push(2);
    return None;
}

fn completion_true(values: &mut Vec<u32>) -> bool {
    values.push(1);
    values.push(2);
    return true;
}

fn completion_error(values: &mut Vec<u32>) -> Result<(), &'static str> {
    values.push(1);
    values.push(2);
    return Err("finished");
}

fn immutable_address_is_not_an_output(value: u32) -> u32 {
    black_box(1);
    black_box(2);
    black_box(3);
    black_box(&raw const value);

    return value;
}

fn deferred_address_is_not_an_output(mut value: u32) -> u32 {
    black_box(1);
    black_box(2);
    black_box(3);

    let _fill_later = || &raw mut value;

    return value;
}

fn mutable_method_return() -> Vec<u8> {
    let mut json = b"{}".to_vec();
    json.reserve(16);
    json.shrink_to_fit();
    json.push(b'\n');
    return json;
}

fn truncate_return(mut output: Vec<u8>, written: usize) -> Vec<u8> {
    output.reserve(16);
    output.push(0);
    output.push(0);
    output.truncate(written);
    return output;
}

fn write_paths(
    directory: &std::path::Path,
    schemas: &[String],
) -> std::io::Result<Vec<std::path::PathBuf>> {
    schemas
        .iter()
        .map(|schema| {
            let path = directory.join(schema);
            std::fs::write(&path, schema).map_err(|error| error)?;
            return Ok(path);
        })
        .collect()
}

fn try_guard(first: i32) -> Result<Vec<u8>, i32> {
    if first != 0 {
        check_status(first)?;
    }
    let output = Vec::new();
    return Ok(output);
}

fn logging_guard(first: i32) -> Result<Vec<u8>, i32> {
    if first != 0 {
        eprintln!("operation failed: {first}");
        return Err(first);
    }
    let output = Vec::new();
    return Ok(output);
}

unsafe extern "C" {
    fn configure_encoder(encoder: *mut std::ffi::c_void, option: usize);
}

fn ffi_initialization(encoder: *mut std::ffi::c_void) {
    let encoder = black_box(encoder);
    // SAFETY: this compile-only fixture assumes encoder is a live handle
    // accepted by configure_encoder throughout this function.
    unsafe {
        configure_encoder(encoder, 0);
    }
    let option_as_alt = 0;
    // SAFETY: encoder remains live and option_as_alt is a supported option.
    unsafe {
        configure_encoder(encoder, option_as_alt);
    }
    let mut encoded = std::ptr::null_mut::<u8>();
    black_box(&mut encoded);
}

fn accumulated_schemas(entries: &[String]) {
    let directory = entries;
    let generated = directory.to_vec();
    let registered = generated.len();

    let mut checked_in = std::collections::BTreeSet::new();
    for entry in directory {
        checked_in.insert(entry.clone());
    }
    black_box((registered, checked_in));
}

fn cache_and_return(last_snapshot: &mut Option<Vec<u8>>, mut state: Vec<u8>) -> Option<Vec<u8>> {
    state.reserve(16);
    state.push(1);
    state.push(2);

    *last_snapshot = Some(state.clone());
    return Some(state);
}

fn validate_registry(registry: &[&str]) -> Result<(), &'static str> {
    let mut filenames = std::collections::BTreeSet::new();
    for filename in registry {
        if !filenames.insert(filename) {
            return Err("duplicate filename");
        }
    }
    return Ok(());
}

fn validation_loop_tail(registry: &[&str]) -> Result<(), &'static str> {
    let mut filenames = std::collections::BTreeSet::new();
    for filename in registry {
        if !filenames.insert(filename) {
            return Err("duplicate filename");
        }
    }
    Ok(())
}
