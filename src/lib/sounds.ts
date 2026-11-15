/**
 * Soft sounds made with Web Audio, so there are no audio files to ship:
 *
 * - a gentle three-note chime when a scan finishes;
 * - a warm two-note tone when a cleanup finishes.
 *
 * Everything is sine waves through a low-pass filter with slow attacks and
 * long releases, kept well below the system's own alert sounds. Off in
 * Settings > Appearance. Nothing plays while the window is hidden, such as
 * during a scheduled scan from the menu bar. There is no sound while a scan
 * runs: testers took a low tone for a fault in the machine.
 */

const PREF = "diskpilot.sounds";

export function soundsEnabled(): boolean {
  try {
    return localStorage.getItem(PREF) !== "off";
  } catch {
    return true;
  }
}

export function setSoundsEnabled(on: boolean): void {
  try {
    localStorage.setItem(PREF, on ? "on" : "off");
  } catch {
    /* storage unavailable: the choice lasts until the app quits */
  }
}

let ctx: AudioContext | null = null;
let master: GainNode | null = null;

function audio(): { ctx: AudioContext; out: GainNode } | null {
  if (typeof window === "undefined" || typeof window.AudioContext === "undefined") return null;
  if (!ctx) {
    ctx = new AudioContext();
    master = ctx.createGain();
    master.gain.value = 0.5;
    const soften = ctx.createBiquadFilter();
    soften.type = "lowpass";
    soften.frequency.value = 2400;
    master.connect(soften).connect(ctx.destination);
  }
  if (ctx.state === "suspended") void ctx.resume().catch(() => undefined);
  return master ? { ctx, out: master } : null;
}

function audible(): boolean {
  return (
    soundsEnabled() && (typeof document === "undefined" || document.visibilityState === "visible")
  );
}

/** One soft bell-like note: a sine with its octave, slow in and long out. */
function note(freq: number, at: number, length: number, volume: number) {
  const a = audio();
  if (!a) return;
  const t = a.ctx.currentTime + at;
  const env = a.ctx.createGain();
  env.gain.setValueAtTime(0, t);
  env.gain.linearRampToValueAtTime(volume, t + 0.04);
  env.gain.exponentialRampToValueAtTime(0.0001, t + length);
  env.connect(a.out);
  for (const [mult, level] of [
    [1, 1],
    [2, 0.18],
  ] as const) {
    const osc = a.ctx.createOscillator();
    osc.type = "sine";
    osc.frequency.value = freq * mult;
    const g = a.ctx.createGain();
    g.gain.value = level;
    osc.connect(g).connect(env);
    osc.start(t);
    osc.stop(t + length + 0.05);
  }
}

/** A scan finished: C, E and G, softly, one after the other. */
export function playScanDone(): void {
  if (!audible()) return;
  note(523.25, 0, 1.4, 0.07);
  note(659.25, 0.12, 1.4, 0.06);
  note(783.99, 0.24, 1.8, 0.055);
}

/** A cleanup finished: a low, warm step down. */
export function playCleanupDone(): void {
  if (!audible()) return;
  note(392.0, 0, 1.2, 0.07);
  note(261.63, 0.16, 1.8, 0.07);
}
