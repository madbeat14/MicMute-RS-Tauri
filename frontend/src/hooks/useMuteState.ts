import { useState, useEffect, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import * as cmd from "../lib/commands";

export function useMuteState() {
  const [isMuted, setIsMuted] = useState(false);
  const [peakLevel, setPeakLevel] = useState(0);

  useEffect(() => {
    cmd.getState().then((s) => {
      setIsMuted(s.is_muted);
      setPeakLevel(s.peak_level);
    }).catch(console.error);

    const unlisten = listen<{ is_muted: boolean; peak_level: number }>(
      "state-update",
      (e) => {
        setIsMuted(e.payload.is_muted);
        setPeakLevel(e.payload.peak_level);
      }
    );
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  const toggle = useCallback(async () => {
    try {
      const res = await cmd.toggleMute();
      setIsMuted(res.is_muted);
      setPeakLevel(res.peak_level);
      return res;
    } catch (e) {
      console.error("toggle_mute failed:", e);
      return null;
    }
  }, []);

  const setMute = useCallback(async (muted: boolean) => {
    try {
      const res = await cmd.setMute(muted);
      setIsMuted(res.is_muted);
      setPeakLevel(res.peak_level);
      return res;
    } catch (e) {
      console.error("set_mute failed:", e);
      return null;
    }
  }, []);

  return { isMuted, peakLevel, setPeakLevel, toggle, setMute };
}
