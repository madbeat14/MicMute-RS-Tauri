import type { AppConfig } from "../../types/config";
import { Card } from "./ui/Card";
import { ToggleRow } from "./ui/ToggleRow";
import { SliderField } from "./ui/SliderField";
import * as cmd from "../../lib/commands";

interface Props {
  config: AppConfig;
  updateConfig: (updater: (prev: AppConfig) => AppConfig) => void;
}

export function AudioTab({ config, updateConfig }: Props) {
  const mode = config.audio_mode || "beep";
  const muteSound = config.sound_config?.mute || { file: "", volume: 50 };
  const unmuteSound = config.sound_config?.unmute || { file: "", volume: 50 };
  const muteBeep = config.beep_config?.mute || { freq: 650, duration: 180, count: 2 };
  const unmuteBeep = config.beep_config?.unmute || { freq: 700, duration: 200, count: 1 };

  const browseMute = async () => {
    const path = await cmd.pickAudioFile();
    if (path) {
      updateConfig((prev) => ({
        ...prev,
        sound_config: {
          ...prev.sound_config,
          mute: { ...prev.sound_config.mute, file: path },
        },
      }));
    }
  };

  const browseUnmute = async () => {
    const path = await cmd.pickAudioFile();
    if (path) {
      updateConfig((prev) => ({
        ...prev,
        sound_config: {
          ...prev.sound_config,
          unmute: { ...prev.sound_config.unmute, file: path },
        },
      }));
    }
  };

  const previewSound = (key: string) => {
    cmd.previewAudioFeedback(mode, key, config).catch(console.error);
  };

  return (
    <Card title="Audio Feedback" icon="🔊">
      <ToggleRow
        label="Enable Audio Feedback"
        checked={config.beep_enabled}
        onChange={(v) => updateConfig((p) => ({ ...p, beep_enabled: v }))}
      />
      <div className="radio-group mt-12">
        <label>
          <input
            type="radio"
            name="audio-mode"
            checked={mode === "beep"}
            onChange={() => updateConfig((p) => ({ ...p, audio_mode: "beep" }))}
          /> Beep
        </label>
        <label>
          <input
            type="radio"
            name="audio-mode"
            checked={mode === "custom"}
            onChange={() => updateConfig((p) => ({ ...p, audio_mode: "custom" }))}
          /> Custom WAV
        </label>
      </div>

      {mode === "custom" && (
        <div className="mt-16">
          <div className="audio-panel">
            <label className="field-label">Mute Sound</label>
            <div className="file-input-row">
              <input type="text" className="text-input" readOnly value={muteSound.file} placeholder="No file selected" />
              <button className="btn-sm" onClick={browseMute}>Browse</button>
              <button className="btn-sm" onClick={() => previewSound("mute")}>Preview</button>
            </div>
            <SliderField
              label="Volume"
              value={muteSound.volume}
              min={0} max={100} unit="%"
              className="mt-8"
              onChange={(v) => updateConfig((p) => ({
                ...p,
                sound_config: { ...p.sound_config, mute: { ...p.sound_config.mute, volume: v } },
              }))}
            />
          </div>
          <div className="audio-panel mt-12">
            <label className="field-label">Unmute Sound</label>
            <div className="file-input-row">
              <input type="text" className="text-input" readOnly value={unmuteSound.file} placeholder="No file selected" />
              <button className="btn-sm" onClick={browseUnmute}>Browse</button>
              <button className="btn-sm" onClick={() => previewSound("unmute")}>Preview</button>
            </div>
            <SliderField
              label="Volume"
              value={unmuteSound.volume}
              min={0} max={100} unit="%"
              className="mt-8"
              onChange={(v) => updateConfig((p) => ({
                ...p,
                sound_config: { ...p.sound_config, unmute: { ...p.sound_config.unmute, volume: v } },
              }))}
            />
          </div>
        </div>
      )}

      {mode === "beep" && (
        <div className="mt-16">
          <div className="audio-panel">
            <label className="field-label">Mute Beep</label>
            <div className="grid-3 mt-4">
              <div>
                <label className="hint">Freq (Hz)</label>
                <input type="number" className="number-input" min={100} max={10000}
                  value={muteBeep.freq}
                  onChange={(e) => updateConfig((p) => ({
                    ...p,
                    beep_config: { ...p.beep_config, mute: { ...muteBeep, freq: Number(e.target.value) } },
                  }))}
                />
              </div>
              <div>
                <label className="hint">Dur (ms)</label>
                <input type="number" className="number-input" min={10} max={2000}
                  value={muteBeep.duration}
                  onChange={(e) => updateConfig((p) => ({
                    ...p,
                    beep_config: { ...p.beep_config, mute: { ...muteBeep, duration: Number(e.target.value) } },
                  }))}
                />
              </div>
              <div>
                <label className="hint">Count</label>
                <input type="number" className="number-input" min={1} max={10}
                  value={muteBeep.count}
                  onChange={(e) => updateConfig((p) => ({
                    ...p,
                    beep_config: { ...p.beep_config, mute: { ...muteBeep, count: Number(e.target.value) } },
                  }))}
                />
              </div>
            </div>
            <button className="btn-secondary btn-full mt-8" onClick={() => previewSound("mute")}>
              Preview Mute Beep
            </button>
          </div>
          <div className="audio-panel mt-12">
            <label className="field-label">Unmute Beep</label>
            <div className="grid-3 mt-4">
              <div>
                <label className="hint">Freq (Hz)</label>
                <input type="number" className="number-input" min={100} max={10000}
                  value={unmuteBeep.freq}
                  onChange={(e) => updateConfig((p) => ({
                    ...p,
                    beep_config: { ...p.beep_config, unmute: { ...unmuteBeep, freq: Number(e.target.value) } },
                  }))}
                />
              </div>
              <div>
                <label className="hint">Dur (ms)</label>
                <input type="number" className="number-input" min={10} max={2000}
                  value={unmuteBeep.duration}
                  onChange={(e) => updateConfig((p) => ({
                    ...p,
                    beep_config: { ...p.beep_config, unmute: { ...unmuteBeep, duration: Number(e.target.value) } },
                  }))}
                />
              </div>
              <div>
                <label className="hint">Count</label>
                <input type="number" className="number-input" min={1} max={10}
                  value={unmuteBeep.count}
                  onChange={(e) => updateConfig((p) => ({
                    ...p,
                    beep_config: { ...p.beep_config, unmute: { ...unmuteBeep, count: Number(e.target.value) } },
                  }))}
                />
              </div>
            </div>
            <button className="btn-secondary btn-full mt-8" onClick={() => previewSound("unmute")}>
              Preview Unmute Beep
            </button>
          </div>
        </div>
      )}
    </Card>
  );
}
