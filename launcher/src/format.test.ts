import { describe, expect, it } from "vitest";
import { bytes, duration, progressLine, SpeedMeter } from "./format";

describe("bytes", () => {
  it("picks a readable unit", () => {
    expect(bytes(512)).toBe("512 B");
    expect(bytes(28_000)).toBe("28 KB");
    expect(bytes(27_844_610)).toBe("27.8 MB");
    expect(bytes(621_553_128)).toBe("621.6 MB");
    expect(bytes(1_450_000_000)).toBe("1.45 GB");
  });
});

describe("duration", () => {
  it("formats seconds, minutes and hours", () => {
    expect(duration(42)).toBe("42 s");
    expect(duration(125)).toBe("2 min 05 s");
    expect(duration(3 * 3600 + 7 * 60)).toBe("3 h 07 min");
    expect(duration(Infinity)).toBe("");
  });
});

describe("SpeedMeter", () => {
  it("averages over its window and resets when a download starts over", () => {
    const m = new SpeedMeter(5000);
    m.push(0, 0);
    m.push(1000, 2_000_000);
    m.push(2000, 4_000_000);
    expect(m.bytesPerSecond()).toBe(2_000_000);
    m.push(3000, 100);
    expect(m.bytesPerSecond()).toBe(0);
  });
});

describe("progressLine", () => {
  it("shows size, speed and time left while downloading", () => {
    const line = progressLine({ stage: "downloading", source: "installer", done: 100_000_000, total: 600_000_000 }, 10_000_000);
    expect(line.text).toBe("Downloading the game installer: 100.0 MB of 600.0 MB · 10.0 MB/s · 50 s left");
    expect(line.fraction).toBeCloseTo(1 / 6);
  });

  it("has no fraction while unpacking", () => {
    expect(progressLine({ stage: "unpacking", source: "installer" }, 0).fraction).toBeNull();
  });
});
