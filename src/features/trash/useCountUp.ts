import { useEffect, useRef, useState } from "react";

export function prefersReducedMotion(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

/**
 * A number that eases towards `target` whenever it changes, starting from
 * wherever it is now. It always ends exactly on `target`; with reduced motion
 * it jumps there. `from` is where the first render starts: 0 counts up on
 * mount, `target` shows the value straight away and animates only changes.
 */
export function useCountUp(target: number, durationMs = 650, from = 0): number {
  const [value, setValue] = useState(from);
  const current = useRef(from);

  useEffect(() => {
    const start0 = current.current;
    if (start0 === target || prefersReducedMotion()) {
      current.current = target;
      setValue(target);
      return;
    }
    let frame = 0;
    const start = performance.now();
    const step = (now: number) => {
      const t = Math.min(1, (now - start) / durationMs);
      const eased = 1 - Math.pow(1 - t, 3);
      const next = t >= 1 ? target : start0 + (target - start0) * eased;
      current.current = next;
      setValue(next);
      if (t < 1) frame = requestAnimationFrame(step);
    };
    frame = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frame);
  }, [target, durationMs]);

  return value;
}
