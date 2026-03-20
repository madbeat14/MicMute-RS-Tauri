import type { AppConfig } from "../../types/config";
import * as cmd from "../../lib/commands";

interface Props {
  debugMsg: string;
  config: AppConfig | null;
}

export function Footer({ debugMsg, config }: Props) {
  const handleSave = () => {
    if (config) cmd.updateConfig(config).catch(console.error);
  };

  return (
    <footer className="app-footer">
      <span className="debug-msg">{debugMsg}</span>
      <button className="btn-primary" onClick={handleSave}>Save Settings</button>
    </footer>
  );
}
