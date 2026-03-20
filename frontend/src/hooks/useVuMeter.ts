import { useEffect } from "react";
import * as cmd from "../lib/commands";

export function useVuMeter(
  onPeak: (peak: number) => void,
  intervalMs: number = 50
) {
  useEffect(() => {
    const timer = setInterval(async () => {
      try {
        const s = await cmd.getState();
        onPeak(s.peak_level);
      } catch (_) {}
    }, intervalMs);
    return () => clearInterval(timer);
  }, [onPeak, intervalMs]);
}
