import { useEffect, useRef, useState, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";

interface OsdShowPayload {
  is_muted: boolean;
  duration: number;
  opacity: number;
}

function App() {
  const [visible, setVisible] = useState(false);
  const [isMuted, setIsMuted] = useState(false);
  const [opacity, setOpacity] = useState(0.8);
  const [hiding, setHiding] = useState(false);
  const fadeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const isLight = window.matchMedia("(prefers-color-scheme: light)").matches;

  const showOsd = useCallback((payload: OsdShowPayload) => {
    const { is_muted, duration, opacity: opacityPct } = payload;

    // Clear any pending fade timer
    if (fadeTimerRef.current) {
      clearTimeout(fadeTimerRef.current);
      fadeTimerRef.current = null;
    }

    setIsMuted(is_muted);
    setOpacity((opacityPct ?? 80) / 100);
    setHiding(false);
    setVisible(true);

    // Fade out ~300ms before end
    fadeTimerRef.current = setTimeout(() => {
      setHiding(true);
    }, Math.max(0, duration - 300));
  }, []);

  useEffect(() => {
    const unlisten = listen<OsdShowPayload>("osd-show", (e) => {
      showOsd(e.payload);
    });

    return () => {
      unlisten.then((fn) => fn());
      if (fadeTimerRef.current) {
        clearTimeout(fadeTimerRef.current);
      }
    };
  }, [showOsd]);

  const getIconSrc = () => {
    if (isMuted) {
      return isLight ? "assets/mic_muted_black.svg" : "assets/mic_muted_white.svg";
    }
    return isLight ? "assets/mic_black.svg" : "assets/mic_white.svg";
  };

  if (!visible) return <div className="osd-page" />;

  return (
    <div className="osd-page">
      <div
        className={`osd-card${hiding ? " hiding" : ""}`}
        style={{ opacity }}
      >
        <img className="osd-icon" src={getIconSrc()} alt="mic" />
      </div>
    </div>
  );
}

export default App;
