import { useState, useEffect } from "react";
import type { AppConfig, DeviceDto } from "../../types/config";
import { Card } from "./ui/Card";
import * as cmd from "../../lib/commands";

interface Props {
  config: AppConfig;
  updateConfig: (updater: (prev: AppConfig) => AppConfig) => void;
}

export function DevicesTab({ config, updateConfig }: Props) {
  const [devices, setDevices] = useState<DeviceDto[]>([]);

  useEffect(() => {
    cmd.getCachedDevices().then(setDevices).catch(() => setDevices([]));
  }, []);

  const refreshDevices = async () => {
    const devs = await cmd.getDevices();
    setDevices(devs);
  };

  const handleDeviceChange = async (id: string) => {
    const deviceId = id || null;
    await cmd.setDevice(deviceId).catch(console.error);
    updateConfig((prev) => ({ ...prev, device_id: deviceId }));
  };

  const handleSyncToggle = (devId: string, checked: boolean) => {
    updateConfig((prev) => {
      const syncIds = checked
        ? [...prev.sync_ids, devId]
        : prev.sync_ids.filter((id) => id !== devId);
      return { ...prev, sync_ids: syncIds };
    });
  };

  const syncDevices = devices.filter((d) => d.id !== (config.device_id || ""));

  return (
    <>
      <Card title="Audio Device" icon="🎧">
        <label className="field-label">Microphone</label>
        <select
          className="select-input"
          value={config.device_id || ""}
          onChange={(e) => handleDeviceChange(e.target.value)}
        >
          <option value="">Default Windows Device</option>
          {devices.map((d) => (
            <option key={d.id} value={d.id}>{d.name}</option>
          ))}
        </select>
        <button className="btn-secondary mt-8" onClick={refreshDevices}>
          ⟳ Refresh
        </button>
      </Card>

      <Card title="Sync Devices" icon="🔗" className="mt-12">
        <p className="hint">These devices mute/unmute alongside the primary device.</p>
        <div className="checkbox-list">
          {syncDevices.map((d) => (
            <label key={d.id}>
              <input
                type="checkbox"
                checked={config.sync_ids.includes(d.id)}
                onChange={(e) => handleSyncToggle(d.id, e.target.checked)}
              />
              {d.name}
            </label>
          ))}
        </div>
      </Card>
    </>
  );
}
