import { useCallback } from "react";
import { useVuMeter } from "../../hooks/useVuMeter";
import type { AppConfig } from "../../types/config";

interface Props {
  isMuted: boolean;
  peakLevel: number;
  onPeakUpdate: (peak: number) => void;
  onToggleMute: () => void;
  config: AppConfig | null;
}

export function Header({ isMuted, peakLevel, onPeakUpdate, onToggleMute, config }: Props) {
  const onPeak = useCallback((peak: number) => {
    onPeakUpdate(peak);
  }, [onPeakUpdate]);

  useVuMeter(onPeak, 50);

  const threshold = (config?.persistent_overlay?.sensitivity ?? 5) / 100;
  let barWidth = 0;
  if (peakLevel >= threshold) {
    const above = (peakLevel - threshold) / (1 - threshold);
    barWidth = Math.min(100, Math.pow(Math.min(1, above * 10), 0.5) * 100);
  }

  return (
    <header className="app-header">
      <div className="header-icon">🎤</div>
      <div className="header-info">
        <h1>MicMuteRs</h1>
        <div className="status-bar">
          <span className={`status-badge ${isMuted ? "muted" : "active"}`}>
            {isMuted ? "🔇 Muted" : "🎤 Active"}
          </span>
          <span className="vu-bar-container">
            <span className="vu-bar" style={{ width: `${barWidth}%` }} />
          </span>
        </div>
      </div>
      <button className="btn-icon" onClick={onToggleMute} title="Toggle Mute">
        {isMuted ? "🔇" : "🎤"}
      </button>
    </header>
  );
}
