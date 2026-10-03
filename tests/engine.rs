//! The real engine against a real model.
//!
//! Every test here is `#[ignore]`d: it needs a GGUF file, which the suite never
//! carries. Run with
//! `GCODE_TEST_MODEL=/path/to/model.gguf cargo test --features inference --test engine -- --ignored`.

#![cfg(feature = "inference")]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use gcode::inference::engine::{LlamaEngine, ModelParams};
use gcode::inference::{generate_command, GenParams, InferenceEngine};

fn engine() -> LlamaEngine {
    let path = std::env::var_os("GCODE_TEST_MODEL").map(PathBuf::from);
    let path = path.expect("set GCODE_TEST_MODEL to a .gguf file");
    LlamaEngine::load(&ModelParams {
        name: "test".to_owned(),
        path,
        n_threads: 0,
        n_gpu_layers: 0,
        context_size: 2048,
    })
    .expect("the model loads")
}

#[test]
#[ignore = "needs GCODE_TEST_MODEL"]
fn a_real_model_answers_with_one_command() {
    let engine = std::sync::Arc::new(engine());
    let prompt = "Reply with one shell command and nothing else.\nTask: list all files, including hidden ones, with sizes.\nCommand: ";
    let command = generate_command(
        &(engine as std::sync::Arc<dyn InferenceEngine>),
        prompt,
        &GenParams::default(),
    )
    .expect("a command");
    assert!(!command.contains('\n'), "more than one line: {command:?}");
    assert!(!command.trim().is_empty());
}

#[test]
#[ignore = "needs GCODE_TEST_MODEL"]
fn the_same_prompt_and_seed_give_the_same_answer() {
    let engine = engine();
    let params = GenParams::default();
    let a = engine
        .generate("Command to print the date: ", &params)
        .expect("a");
    let b = engine
        .generate("Command to print the date: ", &params)
        .expect("b");
    assert_eq!(a, b);
}

#[test]
#[ignore = "needs GCODE_TEST_MODEL"]
fn a_short_timeout_is_an_error_and_not_a_partial_answer() {
    let engine = engine();
    let params = GenParams {
        max_tokens: 4096,
        timeout: Duration::from_millis(1),
        ..GenParams::default()
    };
    let started = Instant::now();
    let result = engine.generate("Write a very long essay about shells: ", &params);
    assert!(result.is_err(), "{result:?}");
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
#[ignore = "needs GCODE_TEST_MODEL"]
fn a_prompt_too_long_for_the_context_is_refused() {
    let engine = engine();
    let prompt = "word ".repeat(10_000);
    let result = engine.generate(&prompt, &GenParams::default());
    assert!(result.is_err(), "{result:?}");
}
