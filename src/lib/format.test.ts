import { describe, it, expect } from "vitest";
import { formatHashrate, formatCompact, formatDuration } from "./format";

describe("formatHashrate", () => {
  it("scales to SI units", () => {
    expect(formatHashrate(0)).toBe("0 H/s");
    expect(formatHashrate(500)).toBe("500.0 H/s");
    expect(formatHashrate(1_500)).toBe("1.50 kH/s");
    expect(formatHashrate(6e12)).toBe("6.00 TH/s");
    expect(formatHashrate(1.024e21)).toMatch(/ZH\/s$/);
  });
});

describe("formatCompact", () => {
  it("compacts large numbers", () => {
    expect(formatCompact(144e12)).toBe("144T");
    expect(formatCompact(Infinity)).toBe("—");
  });
});

describe("formatDuration", () => {
  it("picks a coarse unit", () => {
    expect(formatDuration(45)).toBe("45s");
    expect(formatDuration(3 * 3600)).toBe("3.0h");
    expect(formatDuration(3250 * 31_536_000)).toMatch(/y$/);
    expect(formatDuration(-1)).toBe("—");
  });
});
