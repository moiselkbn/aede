/** "1,6 Go", "512 Mo" */
export function formatSize(bytes: number) {
  const format = (n: number) => n.toLocaleString("fr-FR", { maximumFractionDigits: 1 });
  return bytes >= 1e9 ? `${format(bytes / 1e9)} Go` : `${format(bytes / 1e6)} Mo`;
}

/** "1,1 Go sur 4,1 Go": the unit follows the total, written on both sides. */
export function formatProgress(done: number, total: number) {
  const giga = total >= 1e9;
  const unit = giga ? 1e9 : 1e6;
  const format = (n: number) =>
    `${(n / unit).toLocaleString("fr-FR", { maximumFractionDigits: 1 })} ${giga ? "Go" : "Mo"}`;
  return `${format(done)} sur ${format(total)}`;
}

/** "21 Mo/s" (one decimal under 10). */
export function formatSpeed(bytesPerSecond: number) {
  const mb = bytesPerSecond / 1e6;
  return `${mb.toLocaleString("fr-FR", { maximumFractionDigits: mb < 10 ? 1 : 0 })} Mo/s`;
}
