import { beforeEach, describe, expect, it, vi } from "vitest";

const started: number[] = [];
const stopped: number[] = [];

class FakeParam {
  value = 0;
  setValueAtTime() {}
  linearRampToValueAtTime() {}
  exponentialRampToValueAtTime() {}
  cancelScheduledValues() {}
}
class FakeNode {
  gain = new FakeParam();
  frequency = new FakeParam();
  Q = new FakeParam();
  type = "";
  connect<T>(n: T): T {
    return n;
  }
  start(t: number) {
    started.push(t);
  }
  stop(t: number) {
    stopped.push(t);
  }
}
class FakeAudioContext {
  state = "running";
  currentTime = 0;
  destination = new FakeNode();
  createGain = () => new FakeNode();
  createOscillator = () => new FakeNode();
  createBiquadFilter = () => new FakeNode();
  resume = () => Promise.resolve();
}

let visibility = "visible";
Object.defineProperty(document, "visibilityState", { get: () => visibility });
vi.stubGlobal("AudioContext", FakeAudioContext);

const sounds = await import("./sounds");

beforeEach(() => {
  localStorage.clear();
  visibility = "visible";
  started.length = 0;
  stopped.length = 0;
});

describe("sounds", () => {
  it("are on until turned off, and the choice is kept", () => {
    expect(sounds.soundsEnabled()).toBe(true);
    sounds.setSoundsEnabled(false);
    expect(sounds.soundsEnabled()).toBe(false);
    sounds.playScanDone();
    expect(started).toHaveLength(0);
  });

  it("play a chime of three soft notes when a scan finishes", () => {
    sounds.playScanDone();
    // Each note is a tone and its octave.
    expect(started).toHaveLength(6);
  });

  it("stay silent while the window is hidden", () => {
    visibility = "hidden";
    sounds.playScanDone();
    sounds.playCleanupDone();
    expect(started).toHaveLength(0);
  });

  it("make no sound while a scan runs", () => {
    expect(Object.keys(sounds)).not.toContain("startScanSound");
  });
});
