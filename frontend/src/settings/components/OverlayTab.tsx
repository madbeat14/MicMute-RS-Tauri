import type { AppConfig } from "../../types/config";
import { Card } from "./ui/Card";
import { ToggleRow } from "./ui/ToggleRow";
import { SliderField } from "./ui/SliderField";
import { SelectField } from "./ui/SelectField";

interface Props {
  config: AppConfig;
  updateConfig: (updater: (prev: AppConfig) => AppConfig) => void;
}

export function OverlayTab({ config, updateConfig }: Props) {
  const ol = config.persistent_overlay;

  const setOl = (partial: Partial<typeof ol>) =>
    updateConfig((p) => ({
      ...p,
      persistent_overlay: { ...p.persistent_overlay, ...partial },
    }));

  const enabled = ol.enabled;
  const subStyle = {
    opacity: enabled ? 1 : 0.4,
    pointerEvents: (enabled ? "auto" : "none") as React.CSSProperties["pointerEvents"],
  };

  return (
    <Card title="Persistent Overlay" icon="🪟">
      <ToggleRow label="Enable Overlay" checked={ol.enabled}
        onChange={(v) => setOl({ enabled: v })} />
      <div style={subStyle}>
        <ToggleRow label="Show VU Meter Dot" checked={ol.show_vu} className="mt-8"
          onChange={(v) => setOl({ show_vu: v })} />
        <ToggleRow label="Lock Position" checked={ol.locked}
          onChange={(v) => setOl({ locked: v })} />
        <SelectField label="Position" className="mt-8"
          value={ol.position_mode}
          options={["Custom", "Top-Left", "Top-Center", "Top-Right", "Center-Left", "Center", "Center-Right", "Bottom-Left", "Bottom-Center", "Bottom-Right"]}
          onChange={(v) => setOl({ position_mode: v })} />
        <SelectField label="Icon Theme" className="mt-8"
          value={ol.theme}
          options={["Auto", "Dark", "Light"]}
          onChange={(v) => setOl({ theme: v })} />
        <SliderField label="Size" value={ol.scale} min={16} max={256} unit="px" className="mt-8"
          onChange={(v) => setOl({ scale: v })} />
        <SliderField label="Opacity" value={ol.opacity} min={10} max={100} unit="%" className="mt-8"
          onChange={(v) => setOl({ opacity: v })} />
        <SliderField label="VU Sensitivity" value={ol.sensitivity} min={1} max={50} unit="%" className="mt-8"
          onChange={(v) => setOl({ sensitivity: v })} />
      </div>
    </Card>
  );
}
