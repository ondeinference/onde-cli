import { useEffect, useRef, useState } from "react";
import { ipc } from "@/lib/ipc";
import { errorMessage, type BaseModelView, type DownloadMsg, type ProjectDetail } from "@/lib/types";
import { formatBytes, shortPath } from "./format";

type Progress = Extract<DownloadMsg, { event: "progress" }>;

/** How far along the file being fetched is, 0 to 1, or undefined when its size is
 *  not known. A model is ten small files and one of over a gigabyte, so a bar
 *  counted in files would sit near 60% while almost nothing had arrived. */
export function fileFraction(p: Progress): number | undefined {
  return p.file_bytes_total > 0 ? Math.min(1, p.file_bytes_done / p.file_bytes_total) : undefined;
}

/** The project's base model: where it is, or a button to fetch it. */
export function BaseModelCard({ detail, onChanged }: { detail: ProjectDetail; onChanged: () => void }) {
  const [catalog, setCatalog] = useState<BaseModelView[]>([]);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const alive = useRef(true);

  useEffect(() => {
    alive.current = true;
    ipc.baseModels().then(setCatalog).catch(() => {});
    return () => {
      alive.current = false;
    };
  }, []);

  const entry = catalog.find((m) => m.repo_id === detail.base_model_id);

  async function download() {
    setError(null);
    setDownloading(true);
    try {
      await ipc.downloadBaseModel(detail.base_model_id, (msg) => {
        if (!alive.current) return;
        if (msg.event === "progress") setProgress(msg);
        if (msg.event === "failed") setError(msg.message);
      });
    } catch (e) {
      if (alive.current) setError(errorMessage(e));
    } finally {
      if (alive.current) {
        setDownloading(false);
        setProgress(null);
        onChanged();
      }
    }
  }

  return (
    <section className="rounded-lg border border-fog-line px-4 py-3">
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <h2 className="text-sm font-medium">Base model</h2>
          <p className="truncate text-xs text-fog">
            {entry ? `${entry.display_name} · ${entry.params} · ${entry.size_display}` : detail.base_model_id}
          </p>
        </div>
        {detail.base_dir == null && !downloading && entry && (
          <button
            type="button"
            onClick={() => void download()}
            className="cursor-pointer rounded-md border border-fog-line px-2.5 py-1 text-xs font-medium hover:bg-fog-line"
          >
            Download
          </button>
        )}
        {downloading && (
          <button
            type="button"
            onClick={() => void ipc.cancelBaseDownload(detail.base_model_id)}
            className="cursor-pointer rounded-md border border-fog-line px-2.5 py-1 text-xs font-medium hover:bg-fog-line"
          >
            Cancel
          </button>
        )}
      </div>
      {detail.base_dir != null ? (
        <p className="mt-2 truncate font-mono text-xs text-fog" title={detail.base_dir}>
          {shortPath(detail.base_dir)}
        </p>
      ) : downloading ? (
        <div className="mt-2">
          <progress className="h-1.5 w-full" max={1} value={progress ? fileFraction(progress) : undefined} />
          <p className="mt-1 text-xs tabular-nums text-fog">
            {progress
              ? `File ${progress.file_index + 1} of ${progress.total_files}: ${progress.filename}${
                  progress.file_bytes_total ? ` (${formatBytes(progress.file_bytes_done)} of ${formatBytes(progress.file_bytes_total)})` : ""
                }`
              : "Starting…"}
          </p>
        </div>
      ) : (
        <p className="mt-2 text-xs text-fog">
          {entry ? "Not downloaded yet. Fine-tuning needs the weights on this Mac." : "This base model is not in the app's list, so it cannot be downloaded here."}
        </p>
      )}
      {error && (
        <p role="alert" className="mt-2 text-xs text-red-600">
          {error}
        </p>
      )}
    </section>
  );
}
