import type { AppConfig } from "../../types/config";
import { Card } from "./ui/Card";
import { ToggleRow } from "./ui/ToggleRow";
import { SliderField } from "./ui/SliderField";
import { SelectField } from "./ui/SelectField";

interface Props {
  config: AppConfig;
  updateConfig: (updater: (prev: AppConfig) => AppConfig) => void;
}

export function OsdTab({ config, updateConfig }: Props) {
  const osd = config.osd;

  const setOsd = (partial: Partial<typeof osd>) =>
    updateConfig((p) => ({
      ...p,
      osd: { ...p.osd, ...partial },
    }));

  const enabled = osd.enabled;
  const subStyle = {
    opacity: enabled ? 1 : 0.4,
    pointerEvents: (enabled ? "auto" : "none") as React.CSSProperties["pointerEvents"],
  };

  return (
    <Card title="On-Screen Display (OSD)" icon="📺">
      <ToggleRow label="Enable OSD" checked={osd.enabled}
        onChange={(v) => setOsd({ enabled: v })} />
      <div style={subStyle}>
        <SliderField label="Duration" value={osd.duration} min={500} max={5000} step={100} unit="ms" className="mt-8"
          onChange={(v) => setOsd({ duration: v })} />
        <SliderField label="Size" value={osd.size} min={50} max={300} unit="px" className="mt-8"
          onChange={(v) => setOsd({ size: v })} />
        <SliderField label="Opacity" value={osd.opacity} min={10} max={100} unit="%" className="mt-8"
          onChange={(v) => setOsd({ opacity: v })} />
        <SelectField label="Position" className="mt-8"
          value={osd.position}
          options={["Top", "Center", "Bottom"]}
          onChange={(v) => setOsd({ position: v })} />
      </div>
    </Card>
  );
}
