/** "Intel(R) Core(TM) i7-9700K CPU @ 3.60GHz" → "i7-9700K" */
export function shortCpuName(raw: string): string {
  const clean = raw.replace(/\(R\)|\(TM\)/g, "").replace(/\s+/g, " ");
  const m =
    clean.match(/i[3579]-\w+/) ||
    clean.match(/Ryzen \d+ \w+/) ||
    clean.match(/Core Ultra \d+ \w+/) ||
    clean.match(/Xeon \w[\w-]*/) ||
    clean.match(/EPYC \w+/) ||
    clean.match(/Apple M\d\w*/);
  return m ? m[0] : clean.replace(/\s+CPU.*/, "").trim();
}

/** "NVIDIA GeForce RTX 3080" → "RTX 3080" */
export function shortGpuName(raw: string): string {
  const m =
    raw.match(/RTX \w+(\s*Ti)?(\s*SUPER)?/) ||
    raw.match(/GTX \w+(\s*Ti)?/) ||
    raw.match(/RX \w+(\s*XT)?(\s*X)?/) ||
    raw.match(/Arc \w+/) ||
    raw.match(/Apple M\d\w*/) ||
    raw.match(/Radeon Pro \w+/) ||
    raw.match(/A\d{3,4}\b/);
  return m ? m[0] : raw.replace(/NVIDIA |GeForce |AMD |Intel /g, "").trim();
}

export function formatSize(bytes: number): string {
  const gb = bytes / 1024 ** 3;
  if (gb >= 1) return `${gb.toFixed(1)} GB`;
  const mb = bytes / 1024 ** 2;
  return `${mb.toFixed(2)} MB`;
}

export function quantColor(quant: string): string {
  const q = quant.toUpperCase().replace(/^IQ/, "Q").replace(/^MXFP/, "Q");
  if (q === "F16" || q === "BF16" || q === "F32") return "badge-blue";
  if (q.startsWith("Q8")) return "badge-blue";
  if (q.startsWith("Q7")) return "badge-blue";
  if (q.startsWith("Q6")) return "badge-cyan";
  if (q.startsWith("Q5")) return "badge-green";
  if (q.startsWith("Q4")) return "badge-yellow";
  if (q.startsWith("Q3")) return "badge-orange";
  if (q.startsWith("Q2")) return "badge-red";
  if (q.startsWith("Q1")) return "badge-red-dark";
  return "badge-gray";
}

export function quantSortKey(quant: string | null): number {
  if (!quant) return 999;
  const q = quant.toUpperCase().replace(/^IQ/, "Q").replace(/^MXFP/, "Q");
  if (q === "F32") return 0;
  if (q === "BF16") return 1;
  if (q === "F16") return 2;
  const m = q.match(/^Q(\d)/);
  if (m) return 10 - parseInt(m[1]);
  return 50;
}

export function isImatrixFile(filename: string): boolean {
  const lower = filename.toLowerCase();
  return lower.includes("imatrix") || lower.includes("importance_matrix");
}

export function mbToGb(mb: number): string {
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
  return `${mb} MB`;
}
