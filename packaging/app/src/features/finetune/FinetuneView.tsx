import { isRunning } from "./job";
import { isFinished } from "./sweep";
import { JobPanel } from "./JobPanel";
import { RunForm } from "./RunForm";
import type { Job } from "./useJob";

export function FinetuneView({ job }: { job: Job }) {
  // Between two runs of a sweep the outcome of the last is still on screen for
  // an instant. The form stays hidden for the whole sweep.
  const sweepActive = job.sweep !== null && !isFinished(job.sweep);
  const running = isRunning(job.state) || sweepActive;
  return (
    <div className="flex flex-col gap-4">
      <div>
        <h1 className="text-lg font-semibold">Fine-tune</h1>
        <p className="mt-1 text-sm text-fog">Check the gradients, then train a LoRA adapter and export a GGUF.</p>
      </div>
      <JobPanel job={job} />
      {/* Hidden while a job's result is on screen, so the result is what you see. */}
      {(!job.state.info && job.sweep === null) || running ? <RunForm disabled={running} onStart={job.start} onSweep={job.startSweep} /> : null}
    </div>
  );
}
