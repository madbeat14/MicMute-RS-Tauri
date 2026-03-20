const TABS = [
  { id: "devices", label: "Devices" },
  { id: "audio", label: "Audio" },
  { id: "hotkeys", label: "Hotkeys" },
  { id: "overlay", label: "Overlay" },
  { id: "osd", label: "OSD" },
  { id: "system", label: "System & Startup" },
] as const;

export type TabId = (typeof TABS)[number]["id"];

interface Props {
  activeTab: TabId;
  onTabChange: (tab: TabId) => void;
}

export function TabNav({ activeTab, onTabChange }: Props) {
  return (
    <nav className="tabs-nav">
      {TABS.map((tab) => (
        <button
          key={tab.id}
          className={`tab-btn ${activeTab === tab.id ? "active" : ""}`}
          onClick={() => onTabChange(tab.id)}
        >
          {tab.label}
        </button>
      ))}
    </nav>
  );
}
