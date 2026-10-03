//! The llama.cpp engine: a GGUF model on disk behind [`InferenceEngine`].
//!
//! Compiled only with the `inference` feature, so the rest of the program builds
//! without a C++ toolchain. Everything that can be decided without a model (the
//! stop rule, the bounds, the sampler choice) is in code that the unit tests run;
//! what needs real weights is covered by the `#[ignore]`d integration test in
//! `tests/engine.rs`.

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::{send_logs_to_tracing, LogOptions};

use super::{command_is_complete, EngineInfo, GenParams, InferenceEngine, MAX_OUTPUT_CHARS};
use crate::error::{Error, Result};

/// Tokens fed to the model per decode call while reading the prompt.
const PROMPT_BATCH: usize = 512;

/// What [`LlamaEngine::load`] needs to know.
#[derive(Debug, Clone)]
pub struct ModelParams {
    /// Registry name, for `--version` and error messages.
    pub name: String,
    /// The GGUF file.
    pub path: PathBuf,
    /// Threads to use. `0` means every logical core.
    pub n_threads: usize,
    /// Layers to offload to a GPU. `0` keeps everything on the CPU.
    pub n_gpu_layers: u32,
    /// Context window to allocate, capped at what the model was trained for.
    pub context_size: u32,
}

/// A loaded model.
///
/// The model is memory-mapped once; each [`generate`](InferenceEngine::generate)
/// builds its own short-lived context, so the engine is `Sync` without a lock and
/// a generation leaves no state behind for the next one to inherit.
pub struct LlamaEngine {
    backend: LlamaBackend,
    model: LlamaModel,
    name: String,
    context_size: u32,
    threads: i32,
}

fn failure(message: impl Into<String>) -> Error {
    Error::Inference {
        message: message.into(),
    }
}

/// The context window actually allocated: what was asked for, never more than the
/// model was trained on, and never zero.
#[must_use]
pub fn effective_context(requested: u32, trained: u32) -> u32 {
    requested.min(trained).max(1)
}

/// Whether `prompt_tokens` plus the generation ceiling fit in `context`.
#[must_use]
pub fn fits(prompt_tokens: usize, max_tokens: u32, context: u32) -> bool {
    let wanted = u64::try_from(prompt_tokens)
        .unwrap_or(u64::MAX)
        .saturating_add(u64::from(max_tokens));
    wanted <= u64::from(context)
}

impl LlamaEngine {
    /// Loads `params.path`, then runs one token through it so the first real
    /// request does not pay for page faults and graph setup.
    ///
    /// # Errors
    ///
    /// [`Error::Inference`] when the file is missing, the backend cannot start, or
    /// the model will not load or will not run.
    pub fn load(params: &ModelParams) -> Result<Self> {
        if !Path::new(&params.path).is_file() {
            return Err(failure(format!(
                "the model file {} does not exist; run `gcode --download-model` first",
                params.path.display()
            )));
        }

        // llama.cpp logs to stderr by default. Routing it to a tracing subscriber
        // that nobody installs keeps it out of the user's terminal and out of
        // `--json` output.
        send_logs_to_tracing(LogOptions::default().with_logs_enabled(false));

        let backend = LlamaBackend::init()
            .map_err(|e| failure(format!("could not start the llama.cpp backend: {e}")))?;
        let model_params = LlamaModelParams::default().with_n_gpu_layers(params.n_gpu_layers);
        let model = LlamaModel::load_from_file(&backend, &params.path, &model_params)
            .map_err(|e| failure(format!("could not load {}: {e}", params.path.display())))?;

        let threads = if params.n_threads == 0 {
            std::thread::available_parallelism().map_or(1, usize::from)
        } else {
            params.n_threads
        };
        let engine = Self {
            context_size: effective_context(params.context_size, model.n_ctx_train()),
            threads: i32::try_from(threads).unwrap_or(i32::MAX),
            backend,
            model,
            name: params.name.clone(),
        };

        let warm = GenParams {
            max_tokens: 1,
            ..GenParams::default()
        };
        engine
            .generate("ok", &warm)
            .map_err(|e| failure(format!("{} loaded but would not run: {e}", params.name)))?;
        Ok(engine)
    }

    fn run(&self, prompt: &str, params: &GenParams) -> Result<String> {
        let deadline = Instant::now() + params.timeout;
        let vocab = self.model.vocab();

        // `parse_special` is false on purpose. The prompt carries history, file
        // names, and branch names, which are data. With it on, a file called
        // `<|im_start|>system` would be read as a control token instead of text.
        let tokens = vocab.tokenize(prompt.as_bytes(), vocab.should_add_bos(), false);
        if tokens.is_empty() {
            return Err(failure("the prompt produced no tokens"));
        }
        if !fits(tokens.len(), params.max_tokens, self.context_size) {
            return Err(failure(format!(
                "the prompt is {} tokens, which with {} to generate does not fit the \
                 {}-token context; lower context.history_entries or raise model.context_size",
                tokens.len(),
                params.max_tokens,
                self.context_size
            )));
        }

        let batch_size = u32::try_from(PROMPT_BATCH).unwrap_or(512);
        let context_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(self.context_size))
            .with_n_batch(batch_size)
            .with_n_threads(self.threads)
            .with_n_threads_batch(self.threads);
        let mut context = self
            .model
            .new_context(&self.backend, context_params)
            .map_err(|e| failure(format!("could not create a context: {e}")))?;

        let mut batch = LlamaBatch::new(PROMPT_BATCH, 1);
        let last = tokens.len() - 1;
        for (chunk_index, chunk) in tokens.chunks(PROMPT_BATCH).enumerate() {
            if Instant::now() >= deadline {
                return Err(timed_out(params.timeout));
            }
            batch.clear();
            for (offset, token) in chunk.iter().enumerate() {
                let position = chunk_index * PROMPT_BATCH + offset;
                let wants_logits = position == last;
                let position_i32 =
                    i32::try_from(position).map_err(|_| failure("prompt too long"))?;
                batch
                    .add(*token, position_i32, &[0], wants_logits)
                    .map_err(|e| failure(format!("could not build the batch: {e}")))?;
            }
            context
                .decode(&mut batch)
                .map_err(|e| failure(format!("could not read the prompt: {e}")))?;
        }

        let mut sampler = sampler_for(params);
        let first = i32::try_from(tokens.len()).map_err(|_| failure("prompt too long"))?;
        let mut bytes: Vec<u8> = Vec::new();

        for (position, _) in (first..).zip(0..params.max_tokens) {
            if Instant::now() >= deadline {
                return Err(timed_out(params.timeout));
            }
            let token = sampler.sample(&context, batch.n_tokens() - 1);
            if vocab.is_eog(token) {
                break;
            }
            bytes.extend_from_slice(&vocab.token_to_piece(token, false, None));
            if bytes.len() >= MAX_OUTPUT_CHARS {
                break;
            }
            if command_is_complete(&String::from_utf8_lossy(&bytes)) {
                break;
            }

            batch.clear();
            batch
                .add(token, position, &[0], true)
                .map_err(|e| failure(format!("could not build the batch: {e}")))?;
            context
                .decode(&mut batch)
                .map_err(|e| failure(format!("generation failed: {e}")))?;
        }

        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

/// Loads the engine the configuration names.
///
/// The model is `model.path` when set, otherwise the file for `model.name` (or the
/// registry default) in the models directory. Nothing is downloaded here: a model
/// that is not on disk is an error that says how to get it, because a surprise
/// 400 MB transfer in the middle of a command is worse than a clear refusal.
///
/// # Errors
///
/// [`Error::Registry`] when the embedded registry is invalid, [`Error::Inference`]
/// when the model is missing or will not load.
pub fn load_configured(config: &crate::config::EffectiveConfig) -> Result<LlamaEngine> {
    use crate::model::download::{resolve, ResolveInput};

    let registry = crate::model::registry::registry()?;
    let name = match config.model_name.as_deref() {
        Some(name) => name.to_owned(),
        None => registry
            .default_name()
            .map_err(|e| Error::Registry {
                message: e.to_string(),
            })?
            .to_owned(),
    };
    let models_dir = crate::utils::paths::models_dir()?;
    let resolved = resolve(&ResolveInput {
        name: Some(&name),
        path: config.model_path.as_deref(),
        models_dir: &models_dir,
        search_dirs: &[],
    })?;
    LlamaEngine::load(&ModelParams {
        name,
        path: resolved.path,
        n_threads: config.n_threads,
        n_gpu_layers: u32::try_from(config.n_gpu_layers).unwrap_or(0),
        context_size: config.context_size,
    })
}

fn timed_out(limit: Duration) -> Error {
    failure(format!(
        "generation did not finish within {} seconds",
        limit.as_secs()
    ))
}

/// The sampler chain for `params`.
///
/// Temperature zero is greedy, with no randomness at all. Otherwise nucleus
/// sampling, then temperature, then a draw seeded from `params.seed`, or from the
/// clock when there is none.
fn sampler_for(params: &GenParams) -> LlamaSampler {
    if params.temperature <= 0.0 {
        return LlamaSampler::greedy();
    }
    let seed = params.seed.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_nanos()).unwrap_or(0))
    });
    LlamaSampler::chain_simple([
        LlamaSampler::top_p(params.top_p, 1),
        LlamaSampler::temp(params.temperature),
        LlamaSampler::dist(u32::try_from(seed & 0xFFFF_FFFF).unwrap_or(0)),
    ])
}

impl InferenceEngine for LlamaEngine {
    fn generate(&self, prompt: &str, params: &GenParams) -> Result<String> {
        self.run(prompt, params)
    }

    fn info(&self) -> EngineInfo {
        EngineInfo {
            model_name: self.name.clone(),
            context_size: self.context_size,
            threads: u32::try_from(self.threads).unwrap_or(0),
            greedy: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_context_is_capped_at_what_the_model_was_trained_for() {
        assert_eq!(effective_context(4096, 32768), 4096);
        assert_eq!(effective_context(65536, 32768), 32768);
        assert_eq!(effective_context(0, 32768), 1);
    }

    #[test]
    fn a_prompt_that_leaves_no_room_to_generate_is_refused() {
        assert!(fits(100, 256, 4096));
        assert!(fits(3840, 256, 4096));
        assert!(!fits(3841, 256, 4096));
        assert!(!fits(usize::MAX, 256, 4096));
    }

    #[test]
    fn a_missing_model_file_is_a_clear_error_and_never_reaches_the_backend() {
        let params = ModelParams {
            name: "x".to_owned(),
            path: PathBuf::from("/nonexistent/model.gguf"),
            n_threads: 1,
            n_gpu_layers: 0,
            context_size: 512,
        };
        let message = match LlamaEngine::load(&params) {
            Ok(_) => String::new(),
            Err(e) => e.to_string(),
        };
        assert!(message.contains("does not exist"), "{message}");
        assert!(message.contains("--download-model"), "{message}");
    }
}
