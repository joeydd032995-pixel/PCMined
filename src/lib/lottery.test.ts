import { describe, it, expect } from "vitest";
import {
  expectedTimeToBlock,
  isBlockFound,
  logScalePosition,
  oddsPerWindow,
  LOG_DECADES,
} from "./lottery";

describe("expectedTimeToBlock", () => {
  it("matches the BTC sanity check (~3,250 years)", () => {
    // 6 TH/s vs ~1.024 ZH/s at 600s blocks → ~3,250 years.
    const secs = expectedTimeToBlock(1.024e21, 6e12, 600);
    const years = secs / 31_536_000;
    expect(years).toBeGreaterThan(3000);
    expect(years).toBeLessThan(3500);
  });

  it("is infinite when idle", () => {
    expect(expectedTimeToBlock(1e21, 0, 600)).toBe(Infinity);
    expect(expectedTimeToBlock(0, 6e12, 600)).toBe(Infinity);
  });

  it("halves when hashrate doubles", () => {
    const a = expectedTimeToBlock(1e20, 1e9, 600);
    const b = expectedTimeToBlock(1e20, 2e9, 600);
    expect(b).toBeCloseTo(a / 2, 5);
  });
});

describe("isBlockFound", () => {
  it("fires only when best share reaches difficulty", () => {
    expect(isBlockFound(100, 100)).toBe(true);
    expect(isBlockFound(150, 100)).toBe(true);
    expect(isBlockFound(99.9, 100)).toBe(false);
    expect(isBlockFound(100, 0)).toBe(false);
  });
});

describe("logScalePosition", () => {
  it("is 1.0 at the target and 0 far below / when idle", () => {
    expect(logScalePosition(100, 100)).toBeCloseTo(1, 5);
    expect(logScalePosition(0, 100)).toBe(0);
    expect(logScalePosition(100, 0)).toBe(0);
    // 12 decades below the target clamps to 0.
    expect(logScalePosition(100 / 10 ** LOG_DECADES, 100)).toBeCloseTo(0, 5);
  });

  it("places one decade below target at the right fraction", () => {
    // ratio 0.1 → log10 = -1 → (−1+12)/12
    expect(logScalePosition(10, 100)).toBeCloseTo((LOG_DECADES - 1) / LOG_DECADES, 5);
  });
});

describe("oddsPerWindow", () => {
  it("expresses 1-in-N for a daily window", () => {
    const day = 86_400;
    expect(oddsPerWindow(day * 1000, day)).toBeCloseTo(1000, 5);
    expect(oddsPerWindow(Infinity, day)).toBe(Infinity);
  });
});
