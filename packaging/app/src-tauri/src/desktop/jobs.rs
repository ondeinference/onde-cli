//! Running one fine-tune job at a time and telling the webview how it goes.
//!
//! A job is a [`pipeline`] run: train, then merge and export a GGUF. A gradient
//! census is the same run with a learning rate of 0, one epoch and no export, so
//! it is only a different request.
//!
//! The manager owns no Tauri types. The caller hands it a sink, a closure that
//! receives each [`RunProgress`], and the commands wrap a Tauri channel in one.
//! A webview that reloads mid-run attaches a new sink and carries on.

use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use onde_cli::{
    finetune::FineTuneBackend,
    gguf::GgufDtype,
    hf_clone::BASE_MODELS,
    models,
    pipeline::{self, GgufMeta, RunProgress, RunSpec, Stage},
    project::{self, OndeProject},
};
use serde::{Deserialize, Serialize};

use super::{power::KeepAwake, projects::check_repo_id};

/// Receives each event of the running job. Replaced when the webview reattaches.
pub type Sink = Box<dyn Fn(RunProgress) + Send + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    /// One epoch at a learning rate of 0: it trains nothing and only measures
    /// the gradients, which is how a broken backward pass shows up.
    Census,
    /// A training run that ends in a GGUF.
    Run,
}

/// What the Fine-tune tab asks for.
#[derive(Debug, Clone, Deserialize)]
pub struct JobRequest {
    pub repo_id: String,
    pub kind: JobKind,
    /// Run folder name. Ignored for a census, which names itself.
    #[serde(default)]
    pub name: String,
    pub learning_rate: f64,
    pub epochs: usize,
    pub lora_rank: usize,
    pub lora_alpha: f32,
    pub max_seq_len: usize,
    pub dtype: GgufDtype,
}

/// Which backend a job is training on, and why not the default one.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BackendInfo {
    pub backend: FineTuneBackend,
    pub fallback: Option<String>,
}

/// The job in progress, for a webview that has just loaded. `stage` and
/// `backend` are the latest the job reported. A webview that reloads mid-run
/// missed those events and could not tell a merge from training without them.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JobInfo {
    pub repo_id: String,
    pub run: String,
    pub kind: JobKind,
    pub stage: Option<Stage>,
    pub backend: Option<BackendInfo>,
}

struct Active {
    info: JobInfo,
    cancel: Arc<AtomicBool>,
    sink: Arc<Mutex<Option<Sink>>>,
}

type Slot = Arc<Mutex<Option<Active>>>;

#[derive(Default)]
pub struct Jobs {
    /// Shared with the thread that forwards a run's events, which empties it
    /// when the run ends.
    active: Slot,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn is_final(event: &RunProgress) -> bool {
    matches!(
        event,
        RunProgress::Done(_) | RunProgress::Failed { .. } | RunProgress::Cancelled
    )
}

/// The suggested run name: `e2-lr1e-4`, the way the earlier runs were named.
pub fn default_run_name(epochs: usize, learning_rate: f64) -> String {
    format!("e{epochs}-lr{learning_rate:e}")
}

/// Turn a request into a run spec, checking everything a mistake could break.
/// `base_dir` is where the complete base model is, found by the caller.
pub fn build_spec(
    req: &JobRequest,
    project: &OndeProject,
    base_dir: PathBuf,
    now_secs: u64,
) -> Result<RunSpec, String> {
    let census = req.kind == JobKind::Census;
    if !census {
        if !(req.learning_rate > 0.0 && req.learning_rate <= 1.0) {
            return Err("The learning rate must be above 0 and at most 1.".into());
        }
        if !(1..=100).contains(&req.epochs) {
            return Err("Epochs must be between 1 and 100.".into());
        }
    }
    if !(1..=256).contains(&req.lora_rank) {
        return Err("LoRA rank must be between 1 and 256.".into());
    }
    if !(req.lora_alpha > 0.0 && req.lora_alpha.is_finite()) {
        return Err("LoRA alpha must be above 0.".into());
    }
    if !(16..=8192).contains(&req.max_seq_len) {
        return Err("Max sequence length must be between 16 and 8192.".into());
    }
    project::inspect_dataset(&project.dataset_path)
        .map_err(|e| format!("The dataset can't be used: {e:#}"))?;

    let name = if census {
        format!("census-{now_secs}")
    } else if req.name.trim().is_empty() {
        default_run_name(req.epochs, req.learning_rate)
    } else {
        req.name.trim().to_string()
    };
    pipeline::validate_run_name(&name).map_err(|e| format!("{e:#}"))?;

    let model_name = project
        .repo_id
        .rsplit('/')
        .next()
        .unwrap_or("model")
        .to_string();
    let base_name = BASE_MODELS
        .iter()
        .find(|m| m.repo_id == project.base_model_id)
        .map(|m| m.display_name.to_string());
    Ok(RunSpec {
        name,
        base_dir,
        data_path: project.dataset_path.clone(),
        lora_rank: req.lora_rank,
        lora_alpha: req.lora_alpha,
        learning_rate: if census { 0.0 } else { req.learning_rate },
        epochs: if census { 1 } else { req.epochs },
        max_seq_len: req.max_seq_len,
        dtype: req.dtype,
        gguf: GgufMeta {
            name: model_name,
            base_model_repo: Some(project.base_model_id.clone()),
            base_model_name: base_name,
            ..GgufMeta::default()
        },
        export: !census,
        keep_merged: false,
    })
}

impl Jobs {
    /// Ask the running job to stop. Training stops after the current step; a
    /// merge or export that has begun finishes first. Returns whether there was
    /// a job to ask.
    pub fn cancel(&self) -> bool {
        match lock(&self.active).as_ref() {
            Some(active) => {
                active.cancel.store(true, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    /// Send the running job's events to `sink` from now on. `None` when nothing
    /// is running, and the sink is dropped.
    pub fn attach(&self, sink: Sink) -> Option<JobInfo> {
        let active = lock(&self.active);
        let active = active.as_ref()?;
        *lock(&active.sink) = Some(sink);
        Some(active.info.clone())
    }

    /// Start a job. Fails, and starts nothing, when one is already running or the
    /// project is not ready.
    pub fn start(&self, req: &JobRequest, sink: Sink) -> Result<JobInfo, String> {
        check_repo_id(&req.repo_id)?;
        let project = project::load_project(&req.repo_id).map_err(|e| format!("{e:#}"))?;
        let base_dir = models::base_model_dir(&project.base_model_id).ok_or_else(|| {
            format!(
                "The base model {} is not downloaded. Download it from the project page first.",
                project.base_model_id
            )
        })?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let spec = build_spec(req, &project, base_dir, now)?;
        self.start_spec(req.kind, spec, project, sink)
    }

    fn start_spec(
        &self,
        kind: JobKind,
        spec: RunSpec,
        project: OndeProject,
        sink: Sink,
    ) -> Result<JobInfo, String> {
        let mut guard = lock(&self.active);
        if let Some(running) = guard.as_ref() {
            return Err(format!(
                "{} in {} is still running. Wait for it or cancel it first.",
                running.info.run, running.info.repo_id
            ));
        }
        let info = JobInfo {
            repo_id: project.repo_id.clone(),
            run: spec.name.clone(),
            kind,
            stage: None,
            backend: None,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let sink = Arc::new(Mutex::new(Some(sink)));
        // Registered before the run starts, so a second request can never slip in.
        *guard = Some(Active {
            info: info.clone(),
            cancel: cancel.clone(),
            sink: sink.clone(),
        });
        drop(guard);

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        pipeline::start_run(spec, &project, Some(cancel), tx);
        let reason = format!("Onde Desktop: {} in {}", info.run, info.repo_id);
        let slot = self.active.clone();
        std::thread::spawn(move || {
            // Held on this thread for the whole run, so it ends with it.
            let _awake = KeepAwake::begin(&reason);
            forward(rx, &sink, &slot);
        });
        Ok(info)
    }
}

/// Remember the latest stage and backend on the job, for a webview that attaches
/// later.
fn remember(slot: &Slot, event: &RunProgress) {
    let mut guard = lock(slot);
    let Some(active) = guard.as_mut() else { return };
    match event {
        RunProgress::Stage { stage } => active.info.stage = Some(*stage),
        RunProgress::Backend { backend, fallback } => {
            active.info.backend = Some(BackendInfo {
                backend: *backend,
                fallback: fallback.clone(),
            })
        }
        _ => {}
    }
}

/// Pass a run's events to whoever is listening until it ends, then free the slot.
fn forward(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<RunProgress>,
    sink: &Arc<Mutex<Option<Sink>>>,
    slot: &Slot,
) {
    let send = |event: RunProgress| {
        if let Some(sink) = lock(sink).as_ref() {
            sink(event);
        }
    };
    loop {
        match rx.blocking_recv() {
            Some(event) => {
                remember(slot, &event);
                let done = is_final(&event);
                if done {
                    // Freed before the last event goes out, so a webview that
                    // answers it by starting another job is not refused.
                    *lock(slot) = None;
                }
                send(event);
                if done {
                    return;
                }
            }
            // The run's thread ended without saying how.
            None => {
                *lock(slot) = None;
                send(RunProgress::Failed {
                    message: "The run stopped unexpectedly.".into(),
                });
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(root: &std::path::Path) -> OndeProject {
        OndeProject {
            repo_id: "acme/tiny".into(),
            base_model_id: "Qwen/Qwen3-0.6B".into(),
            base_model_dir: String::new(),
            project_dir: root.to_path_buf(),
            dataset_path: root.join("dataset/train.jsonl"),
            runs_dir: root.join("runs"),
        }
    }

    fn with_dataset(root: &std::path::Path) -> OndeProject {
        let p = project(root);
        std::fs::create_dir_all(p.dataset_path.parent().unwrap()).unwrap();
        std::fs::write(
            &p.dataset_path,
            "{\"messages\":[{\"role\":\"user\",\"content\":\"hi\"},{\"role\":\"assistant\",\"content\":\"hello\"}]}\n",
        )
        .unwrap();
        p
    }

    fn request(kind: JobKind) -> JobRequest {
        JobRequest {
            repo_id: "acme/tiny".into(),
            kind,
            name: String::new(),
            learning_rate: 1e-4,
            epochs: 2,
            lora_rank: 8,
            lora_alpha: 16.0,
            max_seq_len: 512,
            dtype: GgufDtype::Q4KM,
        }
    }

    /// `defaultRunName` in `job.ts` must produce these same strings: the header
    /// shows the name before the backend has replied.
    #[test]
    fn default_run_names_match_the_ones_the_webview_predicts() {
        assert_eq!(default_run_name(2, 1e-4), "e2-lr1e-4");
        assert_eq!(default_run_name(3, 5e-5), "e3-lr5e-5");
        assert_eq!(default_run_name(2, 1.5e-4), "e2-lr1.5e-4");
        assert_eq!(default_run_name(1, 0.05), "e1-lr5e-2");
        assert_eq!(default_run_name(1, 1.0), "e1-lr1e0");
    }

    #[test]
    fn a_census_trains_nothing_for_one_epoch_and_exports_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let p = with_dataset(tmp.path());
        let mut req = request(JobKind::Census);
        req.learning_rate = 5.0; // ignored: a census is always lr 0
        req.epochs = 9;
        let spec = build_spec(&req, &p, PathBuf::from("/base"), 1234).unwrap();
        assert_eq!(spec.name, "census-1234");
        assert_eq!(
            (spec.learning_rate, spec.epochs, spec.export),
            (0.0, 1, false)
        );
    }

    #[test]
    fn a_run_keeps_its_settings_and_gets_a_default_name() {
        let tmp = tempfile::tempdir().unwrap();
        let p = with_dataset(tmp.path());
        let spec = build_spec(&request(JobKind::Run), &p, PathBuf::from("/base"), 1).unwrap();
        assert_eq!(spec.name, "e2-lr1e-4");
        assert_eq!(
            (spec.learning_rate, spec.epochs, spec.export),
            (1e-4, 2, true)
        );
        assert_eq!(spec.gguf.name, "tiny");
        assert_eq!(
            spec.gguf.base_model_repo.as_deref(),
            Some("Qwen/Qwen3-0.6B")
        );
        assert_eq!(spec.gguf.base_model_name.as_deref(), Some("Qwen3 0.6B"));
    }

    #[test]
    fn settings_outside_their_range_are_refused_with_a_reason() {
        let tmp = tempfile::tempdir().unwrap();
        let p = with_dataset(tmp.path());
        let refuse = |edit: &dyn Fn(&mut JobRequest)| {
            let mut req = request(JobKind::Run);
            edit(&mut req);
            build_spec(&req, &p, PathBuf::from("/b"), 1).unwrap_err()
        };
        assert!(refuse(&|r| r.learning_rate = 0.0).contains("learning rate"));
        assert!(refuse(&|r| r.learning_rate = f64::NAN).contains("learning rate"));
        assert!(refuse(&|r| r.epochs = 0).contains("Epochs"));
        assert!(refuse(&|r| r.lora_rank = 0).contains("rank"));
        assert!(refuse(&|r| r.lora_alpha = -1.0).contains("alpha"));
        assert!(refuse(&|r| r.max_seq_len = 4).contains("sequence"));
        assert!(refuse(&|r| r.name = "../x".into()).contains("run name"));
    }

    #[test]
    fn a_missing_or_broken_dataset_stops_the_job_before_it_starts() {
        let tmp = tempfile::tempdir().unwrap();
        let p = project(tmp.path());
        let err = build_spec(&request(JobKind::Run), &p, PathBuf::from("/b"), 1).unwrap_err();
        assert!(err.contains("dataset"), "{err}");
    }

    #[test]
    fn only_one_job_runs_at_a_time_and_a_failed_start_frees_the_slot() {
        let tmp = tempfile::tempdir().unwrap();
        let p = with_dataset(tmp.path());
        let jobs = Jobs::default();

        // The base folder does not exist, so the run fails fast and says why.
        let events = Arc::new(Mutex::new(Vec::<RunProgress>::new()));
        let seen = events.clone();
        let spec = build_spec(&request(JobKind::Run), &p, tmp.path().join("no-base"), 1).unwrap();
        let info = jobs
            .start_spec(
                JobKind::Run,
                spec,
                p.clone(),
                Box::new(move |e| lock(&seen).push(e)),
            )
            .unwrap();
        assert_eq!(info.run, "e2-lr1e-4");

        // Wait for the terminal event, then the slot must be free again.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while lock(&events).iter().all(|e| !is_final(e)) {
            assert!(std::time::Instant::now() < deadline, "the run never ended");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(
            matches!(lock(&events).last(), Some(RunProgress::Failed { .. })),
            "{:?}",
            lock(&events)
        );
        assert!(lock(&jobs.active).is_none(), "the slot is free");
        assert!(!jobs.cancel(), "nothing is left to cancel");

        // A second job may start now.
        let spec =
            build_spec(&request(JobKind::Census), &p, tmp.path().join("no-base"), 7).unwrap();
        assert!(
            jobs.start_spec(JobKind::Census, spec, p, Box::new(|_| {}))
                .is_ok()
        );
    }

    #[test]
    fn a_second_start_while_one_runs_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let p = with_dataset(tmp.path());
        let jobs = Jobs::default();
        *lock(&jobs.active) = Some(Active {
            info: JobInfo {
                repo_id: "acme/other".into(),
                run: "e3-lr1e-4".into(),
                kind: JobKind::Run,
                stage: None,
                backend: None,
            },
            cancel: Arc::new(AtomicBool::new(false)),
            sink: Arc::new(Mutex::new(None)),
        });
        let spec = build_spec(&request(JobKind::Run), &p, PathBuf::from("/b"), 1).unwrap();
        let err = jobs
            .start_spec(JobKind::Run, spec, p, Box::new(|_| {}))
            .unwrap_err();
        assert!(
            err.contains("e3-lr1e-4 in acme/other is still running"),
            "{err}"
        );
        assert!(jobs.cancel(), "cancel reaches the running job");
        assert!(
            lock(&jobs.active)
                .as_ref()
                .unwrap()
                .cancel
                .load(Ordering::Relaxed)
        );
    }

    #[test]
    fn a_reattached_webview_gets_the_events_that_follow() {
        let jobs = Jobs::default();
        assert!(jobs.attach(Box::new(|_| {})).is_none());
        let sink = Arc::new(Mutex::new(None::<Sink>));
        *lock(&jobs.active) = Some(Active {
            info: JobInfo {
                repo_id: "acme/tiny".into(),
                run: "r".into(),
                kind: JobKind::Run,
                stage: None,
                backend: None,
            },
            cancel: Arc::new(AtomicBool::new(false)),
            sink: sink.clone(),
        });
        let got = Arc::new(Mutex::new(0));
        let counter = got.clone();
        let info = jobs
            .attach(Box::new(move |_| *lock(&counter) += 1))
            .unwrap();
        assert_eq!(info.run, "r");
        lock(&sink).as_ref().unwrap()(RunProgress::Cancelled);
        assert_eq!(*lock(&got), 1);
    }

    #[test]
    fn the_latest_stage_and_backend_are_kept_for_a_webview_that_attaches_late() {
        let jobs = Jobs::default();
        *lock(&jobs.active) = Some(Active {
            info: JobInfo {
                repo_id: "acme/tiny".into(),
                run: "r".into(),
                kind: JobKind::Run,
                stage: None,
                backend: None,
            },
            cancel: Arc::new(AtomicBool::new(false)),
            sink: Arc::new(Mutex::new(None)),
        });
        remember(
            &jobs.active,
            &RunProgress::Backend {
                backend: FineTuneBackend::Cpu,
                fallback: Some("no metallib".into()),
            },
        );
        remember(
            &jobs.active,
            &RunProgress::Stage {
                stage: Stage::Training,
            },
        );
        remember(
            &jobs.active,
            &RunProgress::Stage {
                stage: Stage::Merging,
            },
        );
        remember(
            &jobs.active,
            &RunProgress::Phase {
                label: "ignored".into(),
            },
        );

        let info = jobs.attach(Box::new(|_| {})).unwrap();
        assert_eq!(info.stage, Some(Stage::Merging), "the newest stage wins");
        assert_eq!(
            info.backend,
            Some(BackendInfo {
                backend: FineTuneBackend::Cpu,
                fallback: Some("no metallib".into())
            })
        );
        // Nothing to remember into when no job is running.
        remember(
            &Slot::default(),
            &RunProgress::Stage {
                stage: Stage::Exporting,
            },
        );
    }
}
