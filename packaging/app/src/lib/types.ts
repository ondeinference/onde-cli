/** Mirrors `desktop::auth::Profile` in src-tauri. */
export interface Profile {
  email: string;
}

/** Mirrors `Provider` in src-tauri/src/desktop/oauth.rs. */
export type OAuthProvider = "google" | "github" | "gitlab" | "apple";

/** Mirrors `CommandError` in src-tauri/src/error.rs. */
export interface CommandError {
  kind:
    | "email_not_confirmed"
    | "unknown_email"
    | "auth"
    | "cancelled"
    | "timed_out"
    | "invalid_choice"
    | "internal";
  message: string;
}

export function errorMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return String(e);
}

/** A cancelled or timed-out sign-in is not a failure worth showing. */
export function isQuietError(e: unknown): boolean {
  const kind = e && typeof e === "object" && "kind" in e ? (e as { kind: unknown }).kind : null;
  return kind === "cancelled" || kind === "timed_out";
}

/** Mirrors `desktop::links::Location`. */
export interface ProjectsLocation {
  kind: "native" | "container" | "linked";
  path: string;
  note: string | null;
}

/** Mirrors `desktop::projects::ProjectSummary`. */
export interface ProjectSummary {
  repo_id: string;
  base_model_id: string;
  created_at: number;
  dataset: "missing" | "invalid" | "ready";
  /** The complete base model is on disk, so a job can start. */
  base_ready: boolean;
  dataset_rows: number | null;
  runs: number;
  latest_run: string | null;
}

/** Mirrors `onde_cli::project::DatasetInfo`. */
export interface DatasetInfo {
  rows: number;
  text_rows: number;
  message_rows: number;
  with_system: number;
  sample: unknown[];
}

/** Mirrors `desktop::projects::DatasetState`. */
export type DatasetState =
  | { state: "missing" }
  | { state: "invalid"; error: string }
  | ({ state: "ready" } & DatasetInfo);

/** Mirrors `onde_cli::pipeline::RunSpec` (the fields the screen reads). */
export interface RunSpec {
  learning_rate: number;
  epochs: number;
  lora_rank: number;
  max_seq_len: number;
  dtype: "f16" | "q8_0" | "q4_k_m";
  export: boolean;
}

export type RunStatus = "running" | "done" | "failed" | "cancelled" | "imported";

/** Mirrors `onde_cli::pipeline::RunRecord`. */
export interface RunRecord {
  name: string;
  spec: RunSpec | null;
  status: RunStatus;
  backend: "mlx" | "cpu" | "metal" | null;
  backend_note: string | null;
  started_at: number;
  finished_at: number | null;
  steps: number;
  skipped: number;
  epoch_loss: number[];
  adapter_path: string | null;
  gguf_path: string | null;
  gguf_size: number | null;
  error: string | null;
}

/** Mirrors `onde_cli::pipeline::StepRecord`. */
export interface StepRecord {
  epoch: number;
  step: number;
  loss: number;
  grad_norm: number | null;
  skipped: "no_masked_tokens" | "non_finite_loss" | "bad_grad_norm" | null;
  t_ms: number;
}

/** Mirrors `desktop::base_models::BaseModelView`. */
export interface BaseModelView {
  repo_id: string;
  display_name: string;
  size_display: string;
  params: string;
  description: string;
  /** Where the complete model is on disk, or null when it is not downloaded. */
  dir: string | null;
}

/** Mirrors `commands::base_models::DownloadMsg`. */
export type DownloadMsg =
  | {
      event: "progress";
      filename: string;
      file_index: number;
      total_files: number;
      file_bytes_done: number;
      file_bytes_total: number;
    }
  | { event: "done" }
  | { event: "failed"; message: string }
  | { event: "cancelled" };

/** Mirrors `desktop::projects::ProjectDetail`. */
export interface ProjectDetail {
  repo_id: string;
  base_model_id: string;
  created_at: number;
  dataset: DatasetState;
  /** Where the complete base model is on disk, or null when it is not. */
  base_dir: string | null;
  runs: RunRecord[];
  has_eval: boolean;
}

export type JobKind = "census" | "run";
export type Stage = "training" | "merging" | "exporting";
export type Backend = "mlx" | "cpu" | "metal";

/** Mirrors `desktop::jobs::JobRequest`. */
export interface JobRequest {
  repo_id: string;
  kind: JobKind;
  name: string;
  learning_rate: number;
  epochs: number;
  lora_rank: number;
  lora_alpha: number;
  max_seq_len: number;
  dtype: "f16" | "q8_0" | "q4_k_m";
}

/** Mirrors `desktop::jobs::JobInfo`. */
export interface JobInfo {
  repo_id: string;
  run: string;
  kind: JobKind;
}

/** Mirrors `onde_cli::pipeline::RunProgress`, which is tagged with `event`. */
export type RunEvent =
  | { event: "stage"; stage: Stage }
  | { event: "backend"; backend: Backend; fallback: string | null }
  | { event: "phase"; label: string }
  | { event: "training"; epoch: number; total_epochs: number; step: number; total_steps: number; loss: number }
  | ({ event: "step" } & StepRecord)
  | { event: "epoch_done"; epoch: number; mean_loss: number }
  | { event: "merge"; layer: number; total: number }
  | { event: "export"; index: number; total: number }
  | ({ event: "done" } & RunRecord)
  | { event: "failed"; message: string }
  | { event: "cancelled" };

