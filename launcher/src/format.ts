import type { Progress } from "./api";

export function bytes(n: number): string {
  if (n >= 1e9) return `${(n / 1e9).toFixed(2)} GB`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(1)} MB`;
  if (n >= 1e3) return `${(n / 1e3).toFixed(0)} KB`;
  return `${n} B`;
}

export function duration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "";
  const s = Math.round(seconds);
  if (s < 60) return `${s} s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} min ${String(s % 60).padStart(2, "0")} s`;
  return `${Math.floor(m / 60)} h ${String(m % 60).padStart(2, "0")} min`;
}

/** Download speed over a sliding window, so one slow second doesn't swing the estimate. */
export class SpeedMeter {
  private samples: { t: number; done: number }[] = [];

  constructor(private readonly windowMs = 5000) {}

  push(t: number, done: number): void {
    const last = this.samples[this.samples.length - 1];
    if (last && done < last.done) this.samples = [];
    this.samples.push({ t, done });
    while (this.samples.length > 2 && t - this.samples[0].t > this.windowMs) this.samples.shift();
  }

  bytesPerSecond(): number {
    if (this.samples.length < 2) return 0;
    const a = this.samples[0];
    const b = this.samples[this.samples.length - 1];
    return b.t > a.t ? ((b.done - a.done) * 1000) / (b.t - a.t) : 0;
  }

  reset(): void {
    this.samples = [];
  }
}

const sourceName = (id: string) => (id === "installer" ? "the game installer" : "update files");
const part = (done: number, total: number) => (total > 0 ? done / total : null);

export function progressLine(p: Progress, bytesPerSecond: number): { text: string; fraction: number | null } {
  switch (p.stage) {
    case "checking":
      return { text: `Checking game files… ${p.done} of ${p.total}`, fraction: part(p.done, p.total) };
    case "downloading": {
      const bits = [`Downloading ${sourceName(p.source)}: ${bytes(p.done)} of ${bytes(p.total)}`];
      if (bytesPerSecond > 0) bits.push(`${bytes(bytesPerSecond)}/s`, `${duration((p.total - p.done) / bytesPerSecond)} left`);
      return { text: bits.join(" · "), fraction: part(p.done, p.total) };
    }
    case "unpacking":
      return { text: `Unpacking ${sourceName(p.source)}…`, fraction: null };
    case "building":
      return { text: `Installing files… ${p.done} of ${p.total}`, fraction: part(p.done, p.total) };
    case "cleaningUp":
      return { text: "Finishing…", fraction: 1 };
  }
}
