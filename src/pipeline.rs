//! One fine-tune run, start to finish: train a LoRA adapter, merge it into the
//! base weights and export a GGUF.
//!
//! This is the sequence the tests and scripts used to spell out by hand. A run
//! lives in `{project}/runs/{name}/`:
//!
//! ```text
//! run.json      what was asked for and how it ended (see [`RunRecord`])
//! steps.jsonl   one [`StepRecord`] per optimizer step, for charts
//! lora/         the adapter
//! merged/       the merged model, removed unless `keep_merged` is set
//! <name>.gguf   the export
//! ```
//!
//! A gradient census is a run with `lr = 0`, one epoch and `export = false`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

use crate::finetune::{FineTuneBackend, FineTuneConfig, FineTuneProgress, SkipReason};
use crate::gguf::{GgufConfig, GgufDtype, GgufProgress};
use crate::merge::{MergeConfig, MergeProgress};
use crate::project::OndeProject;

/// What goes into the GGUF's `general.*` metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GgufMeta {
    pub name: String,
    pub description: Option<String>,
    pub license: Option<String>,
    pub base_model_repo: Option<String>,
    pub base_model_name: Option<String>,
}

/// Everything a run needs. Stored in `run.json`, so the same run can be
/// repeated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunSpec {
    /// Directory name under `runs/`: letters, digits, `.`, `-` and `_`.
    pub name: String,
    pub base_dir: PathBuf,
    pub data_path: PathBuf,
    pub lora_rank: usize,
    pub lora_alpha: f32,
    pub learning_rate: f64,
    pub epochs: usize,
    pub max_seq_len: usize,
    pub dtype: GgufDtype,
    pub gguf: GgufMeta,
    /// False stops after training, which is what a gradient census wants.
    pub export: bool,
    /// Keep `merged/` (the full-precision merged model) after the export.
    pub keep_merged: bool,
}

/// How a run ended, or where it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Done,
    Failed,
    Cancelled,
    /// A run directory made by hand, with no `run.json`.
    Imported,
}

/// The contents of `run.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    pub name: String,
    /// `None` for an [`RunStatus::Imported`] run.
    pub spec: Option<RunSpec>,
    pub status: RunStatus,
    pub backend: Option<FineTuneBackend>,
    /// Why the default backend was not used.
    pub backend_note: Option<String>,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    /// Optimizer steps that changed the weights.
    pub steps: usize,
    /// Steps that were skipped (no masked tokens, non-finite loss or norm).
    pub skipped: usize,
    /// Mean loss of each finished epoch.
    pub epoch_loss: Vec<f32>,
    pub adapter_path: Option<PathBuf>,
    pub gguf_path: Option<PathBuf>,
    pub gguf_size: Option<u64>,
    pub error: Option<String>,
}

impl RunRecord {
    fn new(spec: &RunSpec) -> Self {
        Self {
            name: spec.name.clone(),
            spec: Some(spec.clone()),
            status: RunStatus::Running,
            backend: None,
            backend_note: None,
            started_at: now_secs(),
            finished_at: None,
            steps: 0,
            skipped: 0,
            epoch_loss: Vec::new(),
            adapter_path: None,
            gguf_path: None,
            gguf_size: None,
            error: None,
        }
    }

    /// Loss of the last finished epoch.
    pub fn final_loss(&self) -> Option<f32> {
        self.epoch_loss.last().copied()
    }
}

/// One line of `steps.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepRecord {
    pub epoch: usize,
    pub step: usize,
    pub loss: f32,
    pub grad_norm: Option<f32>,
    pub skipped: Option<SkipReason>,
    /// Milliseconds since the run started.
    pub t_ms: u64,
}

/// The stages of a run, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Training,
    Merging,
    Exporting,
}

/// What a running pipeline reports.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum RunProgress {
    Stage {
        stage: Stage,
    },
    Backend {
        backend: FineTuneBackend,
        fallback: Option<String>,
    },
    /// Loading, tokenising and saving have no numbers worth charting.
    Phase {
        label: String,
    },
    Training {
        epoch: usize,
        total_epochs: usize,
        step: usize,
        total_steps: usize,
        loss: f32,
    },
    Step(StepRecord),
    EpochDone {
        epoch: usize,
        mean_loss: f32,
    },
    Merge {
        layer: usize,
        total: usize,
    },
    Export {
        index: usize,
        total: usize,
    },
    Done(Box<RunRecord>),
    Failed {
        message: String,
    },
    Cancelled,
}

/// Check a run name before it becomes a directory.
pub fn validate_run_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    anyhow::ensure!(
        ok,
        "run name {name:?} must be 1 to 64 characters of letters, digits, '.', '-' or '_', and not start with '.'"
    );
    Ok(())
}

/// The GGUF file name for a run: `{gguf name}-{dtype}.gguf`, made safe.
pub fn gguf_file_name(meta: &GgufMeta, dtype: GgufDtype) -> String {
    let base: String = meta
        .name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let base = if base.is_empty() {
        "model".to_string()
    } else {
        base
    };
    format!("{base}-{}.gguf", dtype.label())
}

/// Start a run on its own thread. Progress arrives on `tx`; the last event is
/// always `Done`, `Failed` or `Cancelled`. Set `cancel` to stop: training stops
/// after the current step, and a merge or export that has started finishes
/// first.
pub fn start_run(
    spec: RunSpec,
    project: &OndeProject,
    cancel: Option<Arc<AtomicBool>>,
    tx: UnboundedSender<RunProgress>,
) {
    let run_dir = project.runs_dir.join(&spec.name);
    std::thread::spawn(move || {
        // A bad name or an existing run is refused before anything is written,
        // so a mistake can never overwrite an earlier run's record.
        if let Err(e) = prepare_dir(&spec, &run_dir) {
            let _ = tx.send(RunProgress::Failed {
                message: format!("{e:#}"),
            });
            return;
        }
        let mut record = RunRecord::new(&spec);
        let outcome = run(&spec, &run_dir, &cancel, &mut record, &tx);
        record.finished_at = Some(now_secs());
        match outcome {
            Ok(Outcome::Finished) => {
                record.status = RunStatus::Done;
                let _ = write_record(&run_dir, &record);
                let _ = tx.send(RunProgress::Done(Box::new(record)));
            }
            Ok(Outcome::Cancelled) => {
                record.status = RunStatus::Cancelled;
                let _ = write_record(&run_dir, &record);
                let _ = tx.send(RunProgress::Cancelled);
            }
            Err(e) => {
                let message = format!("{e:#}");
                record.status = RunStatus::Failed;
                record.error = Some(message.clone());
                let _ = write_record(&run_dir, &record);
                let _ = tx.send(RunProgress::Failed { message });
            }
        }
    });
}

enum Outcome {
    Finished,
    Cancelled,
}

/// Validate the name and create `{run_dir}/lora`. Fails if the run exists.
fn prepare_dir(spec: &RunSpec, run_dir: &Path) -> Result<()> {
    validate_run_name(&spec.name)?;
    if let Some(runs_dir) = run_dir.parent() {
        std::fs::create_dir_all(runs_dir)
            .with_context(|| format!("creating {}", runs_dir.display()))?;
    }
    // `create_dir` fails when the directory exists, so of two runs started with
    // the same name at the same moment exactly one gets it. Checking `exists()`
    // first and creating after would let both through.
    match std::fs::create_dir(run_dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            anyhow::bail!("run {:?} already exists; pick another name", spec.name)
        }
        Err(e) => return Err(e).with_context(|| format!("creating {}", run_dir.display())),
    }
    let lora_dir = run_dir.join("lora");
    std::fs::create_dir(&lora_dir).with_context(|| format!("creating {}", lora_dir.display()))
}

fn is_cancelled(cancel: &Option<Arc<AtomicBool>>) -> bool {
    cancel.as_ref().is_some_and(|f| f.load(Ordering::Relaxed))
}

fn run(
    spec: &RunSpec,
    run_dir: &Path,
    cancel: &Option<Arc<AtomicBool>>,
    record: &mut RunRecord,
    tx: &UnboundedSender<RunProgress>,
) -> Result<Outcome> {
    let lora_dir = run_dir.join("lora");
    write_record(run_dir, record)?;

    // ---- train ----
    let _ = tx.send(RunProgress::Stage {
        stage: Stage::Training,
    });
    let adapter_path = match train(spec, run_dir, &lora_dir, cancel, record, tx)? {
        Some(path) => path,
        None => return Ok(Outcome::Cancelled),
    };
    record.adapter_path = Some(adapter_path.clone());
    write_record(run_dir, record)?;
    if !spec.export {
        return Ok(Outcome::Finished);
    }

    let outcome = merge_and_export(spec, run_dir, adapter_path, cancel, record, tx);
    // On every way out: a cancelled or failed run would otherwise leave a
    // full-precision merged model (many GB) that nothing records or cleans up.
    if !spec.keep_merged {
        remove_merged(run_dir);
    }
    outcome
}

/// Delete `merged/`, best effort. The GGUF is already written when this runs on
/// success, so a removal that fails must not turn a finished export into a
/// failed run. It is only worth a note on stderr.
fn remove_merged(run_dir: &Path) {
    let merged_dir = run_dir.join("merged");
    if let Err(e) = std::fs::remove_dir_all(&merged_dir)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!("[pipeline] could not remove {}: {e}", merged_dir.display());
    }
}

/// Merge the adapter into the base weights, then export a GGUF.
fn merge_and_export(
    spec: &RunSpec,
    run_dir: &Path,
    adapter_path: PathBuf,
    cancel: &Option<Arc<AtomicBool>>,
    record: &mut RunRecord,
    tx: &UnboundedSender<RunProgress>,
) -> Result<Outcome> {
    // ---- merge ----
    if is_cancelled(cancel) {
        return Ok(Outcome::Cancelled);
    }
    let _ = tx.send(RunProgress::Stage {
        stage: Stage::Merging,
    });
    let merged_dir = run_dir.join("merged");
    let (mg_tx, mut mg_rx) = tokio::sync::mpsc::unbounded_channel();
    crate::merge::start_merge(
        MergeConfig {
            base_dir: spec.base_dir.clone(),
            adapter_path,
            output_dir: merged_dir.clone(),
        },
        mg_tx,
    );
    loop {
        match mg_rx.blocking_recv() {
            Some(MergeProgress::Merging { layer, total }) => {
                let _ = tx.send(RunProgress::Merge { layer, total });
            }
            Some(MergeProgress::Done { .. }) => break,
            Some(MergeProgress::Failed(e)) => anyhow::bail!("merge failed: {e}"),
            Some(_) => {}
            None => anyhow::bail!("the merge thread ended without a result"),
        }
    }

    // ---- export ----
    if is_cancelled(cancel) {
        return Ok(Outcome::Cancelled);
    }
    let _ = tx.send(RunProgress::Stage {
        stage: Stage::Exporting,
    });
    let gguf_path = run_dir.join(gguf_file_name(&spec.gguf, spec.dtype));
    let (gg_tx, mut gg_rx) = tokio::sync::mpsc::unbounded_channel();
    crate::gguf::start_gguf_export(
        GgufConfig {
            model_dir: merged_dir.clone(),
            output_path: gguf_path.clone(),
            dtype: spec.dtype,
            name: spec.gguf.name.clone(),
            description: spec.gguf.description.clone(),
            license: spec.gguf.license.clone(),
            base_model_repo: spec.gguf.base_model_repo.clone(),
            base_model_name: spec.gguf.base_model_name.clone(),
        },
        gg_tx,
    );
    loop {
        match gg_rx.blocking_recv() {
            Some(GgufProgress::WritingTensor { index, total, .. }) => {
                let _ = tx.send(RunProgress::Export { index, total });
            }
            Some(GgufProgress::Done { size_bytes, .. }) => {
                record.gguf_path = Some(gguf_path);
                record.gguf_size = Some(size_bytes);
                break;
            }
            Some(GgufProgress::Failed(e)) => anyhow::bail!("GGUF export failed: {e}"),
            Some(_) => {}
            None => anyhow::bail!("the export thread ended without a result"),
        }
    }
    Ok(Outcome::Finished)
}

/// Train and stream the steps. `None` means the run was cancelled.
fn train(
    spec: &RunSpec,
    run_dir: &Path,
    lora_dir: &Path,
    cancel: &Option<Arc<AtomicBool>>,
    record: &mut RunRecord,
    tx: &UnboundedSender<RunProgress>,
) -> Result<Option<PathBuf>> {
    use std::io::Write;

    let (ft_tx, mut ft_rx) = tokio::sync::mpsc::unbounded_channel();
    crate::finetune::start_finetune(
        FineTuneConfig {
            model_dir: spec.base_dir.clone(),
            data_path: spec.data_path.clone(),
            output_dir: lora_dir.to_path_buf(),
            lora_rank: spec.lora_rank,
            lora_alpha: spec.lora_alpha,
            learning_rate: spec.learning_rate,
            epochs: spec.epochs,
            max_seq_len: spec.max_seq_len,
            cancel: cancel.clone(),
        },
        ft_tx,
    );

    let mut steps_file =
        std::fs::File::create(run_dir.join("steps.jsonl")).context("creating steps.jsonl")?;
    let started = Instant::now();
    let mut epoch_sum = 0.0f64;
    let mut epoch_count = 0usize;
    let mut current_epoch = 1usize;

    let close_epoch = |epoch: usize, sum: &mut f64, count: &mut usize, record: &mut RunRecord| {
        if *count > 0 {
            let mean_loss = (*sum / *count as f64) as f32;
            record.epoch_loss.push(mean_loss);
            let _ = tx.send(RunProgress::EpochDone { epoch, mean_loss });
        }
        *sum = 0.0;
        *count = 0;
    };

    loop {
        match ft_rx.blocking_recv() {
            Some(FineTuneProgress::Started { backend, fallback }) => {
                record.backend = Some(backend);
                record.backend_note = fallback.clone();
                let _ = tx.send(RunProgress::Backend { backend, fallback });
            }
            Some(FineTuneProgress::Validating) => phase(tx, "Checking the model and data"),
            Some(FineTuneProgress::LoadingModel) => phase(tx, "Loading the model"),
            Some(FineTuneProgress::Tokenizing { done, total }) => {
                phase(tx, &format!("Tokenizing {done}/{total}"))
            }
            Some(FineTuneProgress::Saving) => phase(tx, "Saving the adapter"),
            Some(FineTuneProgress::Training {
                epoch,
                total_epochs,
                step,
                total_steps,
                loss,
            }) => {
                let _ = tx.send(RunProgress::Training {
                    epoch,
                    total_epochs,
                    step,
                    total_steps,
                    loss,
                });
            }
            Some(FineTuneProgress::Step {
                epoch,
                step,
                loss,
                grad_norm,
                skipped,
            }) => {
                if epoch != current_epoch {
                    close_epoch(current_epoch, &mut epoch_sum, &mut epoch_count, record);
                    current_epoch = epoch;
                }
                match skipped {
                    None => {
                        record.steps += 1;
                        epoch_sum += loss as f64;
                        epoch_count += 1;
                    }
                    Some(_) => record.skipped += 1,
                }
                let step_record = StepRecord {
                    epoch,
                    step,
                    loss,
                    grad_norm,
                    skipped,
                    t_ms: started.elapsed().as_millis() as u64,
                };
                if let Ok(line) = serde_json::to_string(&step_record) {
                    let _ = writeln!(steps_file, "{line}");
                }
                let _ = tx.send(RunProgress::Step(step_record));
            }
            Some(FineTuneProgress::Done { adapter_path }) => {
                close_epoch(current_epoch, &mut epoch_sum, &mut epoch_count, record);
                return Ok(Some(adapter_path));
            }
            Some(FineTuneProgress::Cancelled) => return Ok(None),
            Some(FineTuneProgress::Failed(e)) => anyhow::bail!("{e}"),
            None => anyhow::bail!("the training thread ended without a result"),
        }
    }
}

fn phase(tx: &UnboundedSender<RunProgress>, label: &str) {
    let _ = tx.send(RunProgress::Phase {
        label: label.to_string(),
    });
}

fn write_record(run_dir: &Path, record: &RunRecord) -> Result<()> {
    let json = serde_json::to_string_pretty(record).context("serializing run.json")?;
    // Write beside it and rename. A reader (`list_runs` while the run is going)
    // sees the old file or the new one, never a truncated one, and a crash
    // cannot leave a half-written record.
    let tmp = run_dir.join("run.json.tmp");
    std::fs::write(&tmp, json).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, run_dir.join("run.json"))
        .with_context(|| format!("writing {}/run.json", run_dir.display()))
}

/// Read `run.json` from a run directory.
pub fn read_record(run_dir: &Path) -> Result<RunRecord> {
    let path = run_dir.join("run.json");
    let raw =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("parsing {}", path.display()))
}

/// Read `steps.jsonl`. Lines that do not parse (a run still writing) are
/// skipped.
pub fn read_steps(run_dir: &Path) -> Vec<StepRecord> {
    let Ok(raw) = std::fs::read_to_string(run_dir.join("steps.jsonl")) else {
        return Vec::new();
    };
    raw.lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str) -> RunSpec {
        RunSpec {
            name: name.to_string(),
            base_dir: PathBuf::from("/nonexistent/base"),
            data_path: PathBuf::from("/nonexistent/train.jsonl"),
            lora_rank: 8,
            lora_alpha: 16.0,
            learning_rate: 1e-4,
            epochs: 2,
            max_seq_len: 512,
            dtype: GgufDtype::Q4KM,
            gguf: GgufMeta {
                name: "siMail v2".to_string(),
                ..GgufMeta::default()
            },
            export: true,
            keep_merged: false,
        }
    }

    #[test]
    fn run_names_are_safe_directory_names() {
        for ok in ["e2-lr1e-4", "v2.census_0", "A"] {
            assert!(validate_run_name(ok).is_ok(), "{ok}");
        }
        for bad in ["", ".hidden", "a/b", "../x", "has space", &"x".repeat(65)] {
            assert!(validate_run_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn gguf_file_names_carry_the_dtype_and_stay_safe() {
        let meta = GgufMeta {
            name: "siMail v2".to_string(),
            ..GgufMeta::default()
        };
        assert_eq!(
            gguf_file_name(&meta, GgufDtype::Q4KM),
            "siMail-v2-q4_k_m.gguf"
        );
        assert_eq!(
            gguf_file_name(&GgufMeta::default(), GgufDtype::F16),
            "model-f16.gguf"
        );
    }

    #[test]
    fn a_spec_survives_run_json() {
        let tmp = tempfile::tempdir().unwrap();
        let mut record = RunRecord::new(&spec("a"));
        record.epoch_loss = vec![0.97, 0.48];
        record.backend = Some(FineTuneBackend::Mlx);
        write_record(tmp.path(), &record).unwrap();
        assert_eq!(read_record(tmp.path()).unwrap(), record);
        assert_eq!(record.final_loss(), Some(0.48));
    }

    #[test]
    fn steps_that_do_not_parse_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let good = StepRecord {
            epoch: 1,
            step: 1,
            loss: 2.5,
            grad_norm: Some(41.0),
            skipped: None,
            t_ms: 12,
        };
        let skipped = StepRecord {
            skipped: Some(SkipReason::BadGradNorm),
            ..good.clone()
        };
        let text = format!(
            "{}\n{{\"epoch\":1,\n{}\n",
            serde_json::to_string(&good).unwrap(),
            serde_json::to_string(&skipped).unwrap()
        );
        std::fs::write(tmp.path().join("steps.jsonl"), text).unwrap();
        assert_eq!(read_steps(tmp.path()), vec![good, skipped]);
    }

    /// A run that cannot start reports `Failed`, leaves a failed `run.json`,
    /// and never overwrites an existing run.
    #[test]
    fn a_missing_model_fails_the_run_and_is_recorded() {
        let tmp = tempfile::tempdir().unwrap();
        let project = OndeProject {
            repo_id: "o/p".to_string(),
            base_model_id: "b".to_string(),
            base_model_dir: String::new(),
            project_dir: tmp.path().to_path_buf(),
            dataset_path: tmp.path().join("dataset/train.jsonl"),
            runs_dir: tmp.path().join("runs"),
        };
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        start_run(spec("broken"), &project, None, tx);
        let last = std::iter::from_fn(|| rx.blocking_recv()).last().unwrap();
        assert!(matches!(last, RunProgress::Failed { .. }), "{last:?}");
        let record = read_record(&project.runs_dir.join("broken")).unwrap();
        assert_eq!(record.status, RunStatus::Failed);
        assert!(record.error.is_some());

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        start_run(spec("broken"), &project, None, tx);
        match std::iter::from_fn(|| rx.blocking_recv()).last().unwrap() {
            RunProgress::Failed { message } => {
                assert!(message.contains("already exists"), "{message}")
            }
            other => panic!("{other:?}"),
        }
        // The first run's record is untouched.
        assert_eq!(
            read_record(&project.runs_dir.join("broken"))
                .unwrap()
                .status,
            RunStatus::Failed
        );
    }

    #[test]
    fn run_json_is_replaced_without_leaving_a_temp_file() {
        let tmp = tempfile::tempdir().unwrap();
        let mut record = RunRecord::new(&spec("a"));
        write_record(tmp.path(), &record).unwrap();
        record.steps = 7;
        write_record(tmp.path(), &record).unwrap();
        assert_eq!(read_record(tmp.path()).unwrap().steps, 7);
        assert!(!tmp.path().join("run.json.tmp").exists());
    }

    #[test]
    fn the_merged_model_is_removed_and_a_missing_one_is_fine() {
        let tmp = tempfile::tempdir().unwrap();
        remove_merged(tmp.path()); // nothing there: no panic, no error
        let merged = tmp.path().join("merged");
        std::fs::create_dir_all(&merged).unwrap();
        std::fs::write(merged.join("model.safetensors"), b"x").unwrap();
        remove_merged(tmp.path());
        assert!(!merged.exists());
    }

    #[test]
    fn two_runs_with_one_name_cannot_both_get_the_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("runs/same");
        let s = spec("same");
        assert!(prepare_dir(&s, &dir).is_ok());
        let again = prepare_dir(&s, &dir).unwrap_err();
        assert!(format!("{again:#}").contains("already exists"), "{again:#}");
    }

    /// Hosts send these as JSON. An internally tagged enum cannot serialize a
    /// newtype variant that holds a plain string, so every variant is checked.
    #[test]
    fn every_progress_event_serializes_with_its_event_tag() {
        let record = RunRecord::new(&spec("a"));
        let step = StepRecord {
            epoch: 1,
            step: 2,
            loss: 0.5,
            grad_norm: Some(3.0),
            skipped: None,
            t_ms: 4,
        };
        let events = vec![
            (
                RunProgress::Stage {
                    stage: Stage::Training,
                },
                "stage",
            ),
            (
                RunProgress::Backend {
                    backend: FineTuneBackend::Mlx,
                    fallback: None,
                },
                "backend",
            ),
            (RunProgress::Phase { label: "x".into() }, "phase"),
            (
                RunProgress::Training {
                    epoch: 1,
                    total_epochs: 2,
                    step: 3,
                    total_steps: 4,
                    loss: 1.0,
                },
                "training",
            ),
            (RunProgress::Step(step), "step"),
            (
                RunProgress::EpochDone {
                    epoch: 1,
                    mean_loss: 0.1,
                },
                "epoch_done",
            ),
            (RunProgress::Merge { layer: 1, total: 2 }, "merge"),
            (RunProgress::Export { index: 1, total: 2 }, "export"),
            (RunProgress::Done(Box::new(record)), "done"),
            (
                RunProgress::Failed {
                    message: "boom".into(),
                },
                "failed",
            ),
            (RunProgress::Cancelled, "cancelled"),
        ];
        for (event, tag) in events {
            let json = serde_json::to_value(&event)
                .unwrap_or_else(|e| panic!("{tag} does not serialize: {e}"));
            assert_eq!(json["event"], tag, "{json}");
        }
    }
}
