// The lottery math, rendered honestly.
//
//   E[time to block] = avg_block_interval × (network_hashrate / your_hashrate)
//
// A block is "found" when your best share difficulty reaches the network
// difficulty. Because the gap spans many orders of magnitude, the UI renders
// best-share-vs-difficulty on a LOG scale.

export const LOG_DECADES = 12; // how many orders of magnitude the bar spans

/** Expected seconds until this hashrate finds a block. Infinity if idle. */
export function expectedTimeToBlock(
  networkHashrate: number,
  yourHashrate: number,
  blockTime: number,
): number {
  if (yourHashrate <= 0 || networkHashrate <= 0 || blockTime <= 0) return Infinity;
  return blockTime * (networkHashrate / yourHashrate);
}

/** True when a share has reached network difficulty — a found block. */
export function isBlockFound(bestShareDiff: number, networkDifficulty: number): boolean {
  return networkDifficulty > 0 && bestShareDiff >= networkDifficulty;
}

/**
 * Position (0..1) of the best share on a log scale toward network difficulty.
 * 1.0 means a block was found; values span {@link LOG_DECADES} decades below.
 */
export function logScalePosition(bestShareDiff: number, networkDifficulty: number): number {
  if (networkDifficulty <= 0 || bestShareDiff <= 0) return 0;
  const ratio = bestShareDiff / networkDifficulty;
  const log = Math.log10(ratio); // 0 at the target, negative below
  const clamped = Math.max(-LOG_DECADES, Math.min(0, log));
  return (clamped + LOG_DECADES) / LOG_DECADES;
}

/** Odds of finding a block in a given window, as "1 in N". */
export function oddsPerWindow(expectedSecs: number, windowSecs: number): number {
  if (!isFinite(expectedSecs) || expectedSecs <= 0) return Infinity;
  return expectedSecs / windowSecs;
}
