import { useState, useEffect } from "react";
import type { AppConfig } from "../../types/config";
import { Card } from "./ui/Card";
import { ToggleRow } from "./ui/ToggleRow";
import { SliderField } from "./ui/SliderField";
import * as cmd from "../../lib/commands";

interface Props {
  config: AppConfig;
  updateConfig: (updater: (prev: AppConfig) => AppConfig) => void;
}

export function SystemTab({ config, updateConfig }: Props) {
  const [runOnStartup, setRunOnStartup] = useState(false);

  useEffect(() => {
    cmd.getRunOnStartup().then(setRunOnStartup).catch(() => {});
  }, []);

  const toggleStartup = async (enabled: boolean) => {
    await cmd.setRunOnStartup(enabled).catch(console.error);
    setRunOnStartup(enabled);
  };

  const afkEnabled = config.afk.enabled;
  const subStyle = {
    opacity: afkEnabled ? 1 : 0.4,
    pointerEvents: (afkEnabled ? "auto" : "none") as React.CSSProperties["pointerEvents"],
  };

  return (
    <Card title="System & Startup" icon="⚙️">
      <ToggleRow label="Start MicMuteRs on Boot" checked={runOnStartup}
        onChange={toggleStartup} />
      <ToggleRow label="Enable AFK Auto-Mute" checked={config.afk.enabled} className="mt-8"
        onChange={(v) => updateConfig((p) => ({ ...p, afk: { ...p.afk, enabled: v } }))} />
      <div style={subStyle}>
        <SliderField
          label="Timeout"
          value={config.afk.timeout}
          min={5} max={3600} unit="s" className="mt-8"
          onChange={(v) => updateConfig((p) => ({ ...p, afk: { ...p.afk, timeout: v } }))}
        />
      </div>
      <div className="link-row mt-8">
        <a href="#" onClick={(e) => { e.preventDefault(); cmd.openUrl("https://github.com/madbeat14/MicMuteRS"); }}>
          📖 GitHub
        </a>
      </div>
    </Card>
  );
}
