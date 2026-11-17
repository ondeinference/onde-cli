export function formatBytes(bytes: number | null): string {
  if (bytes == null) return "";
  const gb = bytes / 1024 ** 3;
  if (gb >= 1) return `${gb.toFixed(2)} GB`;
  return `${Math.round(bytes / 1024 ** 2)} MB`;
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
