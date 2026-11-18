export function formatBytes(bytes: number | null): string {
  if (bytes == null) return "";
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${bytes} B`;
}

export function formatDate(unixSeconds: number): string {
  if (!unixSeconds) return "";
  return new Date(unixSeconds * 1000).toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

/** 0.0001 reads as "1e-4", the way the runs are named. */
export function formatLr(lr: number): string {
  if (lr === 0) return "0";
  return lr >= 0.01 ? String(lr) : lr.toExponential();
}

export function formatLoss(loss: number | undefined): string {
  return loss == null ? "" : loss.toFixed(3);
}

/** The home directory as `~`, so paths fit and read the way people write them. */
export function shortPath(path: string): string {
  return path.replace(/^\/Users\/[^/]+/, "~");
}
