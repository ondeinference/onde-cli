// Why a setting blocks Start, in words. The limits mirror `build_spec` in the
// backend (`desktop/jobs.rs`), so the form says what the backend would say, and
// says it before the click instead of leaving Start quietly disabled.

const toNumber = (text: string): number => (text.trim() === "" ? NaN : Number(text));
const isWhole = (n: number, lo: number, hi: number): boolean => Number.isInteger(n) && n >= lo && n <= hi;

/** The LoRA settings live in a disclosure that is closed by default, so a bad
 *  value there is invisible. `null` means all three are fine. */
export function loraProblem(rank: string, alpha: string, maxSeq: string): string | null {
  if (!isWhole(toNumber(rank), 1, 256)) return "LoRA rank must be a whole number from 1 to 256.";
  const a = toNumber(alpha);
  if (!(Number.isFinite(a) && a > 0)) return "LoRA alpha must be a number above 0.";
  if (!isWhole(toNumber(maxSeq), 16, 8192)) return "Max sequence must be a whole number from 16 to 8192.";
  return null;
}

/** The learning rate and epochs of a single training run. `null` means both are fine. */
export function runProblem(learningRate: string, epochs: string): string | null {
  const lr = toNumber(learningRate);
  if (!(Number.isFinite(lr) && lr > 0 && lr <= 1)) return "Learning rate must be above 0 and at most 1, like 1e-4.";
  if (!isWhole(toNumber(epochs), 1, 100)) return "Epochs must be a whole number from 1 to 100.";
  return null;
}
