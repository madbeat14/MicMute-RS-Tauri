import { useState, useEffect, useCallback } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { useConfig } from "../hooks/useConfig";
import { useMuteState } from "../hooks/useMuteState";
import { jsEventToVK } from "../lib/hotkeys";
import { Header } from "./components/Header";
import { TabNav, type TabId } from "./components/TabNav";
import { Footer } from "./components/Footer";
import { DevicesTab } from "./components/DevicesTab";
import { AudioTab } from "./components/AudioTab";
import { HotkeysTab } from "./components/HotkeysTab";
import { OverlayTab } from "./components/OverlayTab";
import { OsdTab } from "./components/OsdTab";
import { SystemTab } from "./components/SystemTab";

export default function App() {
  const { config, updateConfig } = useConfig();
  const { isMuted, peakLevel, setPeakLevel, toggle, setMute } = useMuteState();
  const [activeTab, setActiveTab] = useState<TabId>("devices");
  const [debugMsg, setDebugMsg] = useState("");

  // Auto-fit window on first render
  useEffect(() => {
    if (!config) return;
    const fit = async () => {
      await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
      const content = document.querySelector(".settings-content") as HTMLElement;
      const header = document.querySelector(".app-header") as HTMLElement;
      const tabs = document.querySelector(".tabs-nav") as HTMLElement;
      const footer = document.querySelector(".app-footer") as HTMLElement;
      if (!content || !header || !tabs || !footer) return;

      // Temporarily show all tabs to find the tallest
      const panes = content.querySelectorAll<HTMLElement>(".tab-pane");
      const prev = content.style.flex;
      const prevOv = content.style.overflow;
      content.style.flex = "0 0 auto";
      content.style.overflow = "visible";
      let maxH = 0;
      panes.forEach((pane) => {
        const wasActive = pane.classList.contains("active");
        if (!wasActive) pane.classList.add("active");
        if (pane.scrollHeight > maxH) maxH = pane.scrollHeight;
        if (!wasActive) pane.classList.remove("active");
      });
      content.style.flex = prev;
      content.style.overflow = prevOv;

      // Measure tab row width
      let tabsWidth = 0;
      tabs.querySelectorAll<HTMLElement>(".tab-btn").forEach((btn) => {
        tabsWidth += btn.offsetWidth;
      });
      const tabCount = tabs.querySelectorAll(".tab-btn").length;
      tabsWidth += (tabCount - 1) * 4 + 28;

      const totalH = header.offsetHeight + tabs.offsetHeight + maxH + 24 + footer.offsetHeight;
      const desiredW = Math.max(tabsWidth, 480);
      const screenH = window.screen.availHeight;
      const finalH = Math.min(totalH, Math.floor(screenH * 0.9));

      const win = getCurrentWindow();
      await win.setSize(new LogicalSize(desiredW, finalH));
    };
    fit();
  }, [config]);

  // Hotkey passthrough when settings window is focused
  useEffect(() => {
    if (!config) return;
    const handler = async (e: KeyboardEvent) => {
      const vk = jsEventToVK(e);
      if (!vk) return;

      const mode = config.hotkey_mode;
      let matched = false;

      if (mode === "toggle") {
        const toggleVk = config.hotkey?.toggle?.vk || 0;
        if (toggleVk && vk === toggleVk) {
          matched = true;
          await toggle();
        }
      } else {
        const muteVk = config.hotkey?.mute?.vk || 0;
        const unmuteVk = config.hotkey?.unmute?.vk || 0;
        if (muteVk && muteVk === unmuteVk && vk === muteVk) {
          matched = true;
          await toggle();
        } else if (muteVk && vk === muteVk) {
          matched = true;
          await setMute(true);
        } else if (unmuteVk && vk === unmuteVk) {
          matched = true;
          await setMute(false);
        }
      }

      if (matched) {
        e.preventDefault();
        e.stopPropagation();
      }
    };
    document.addEventListener("keydown", handler, true);
    return () => document.removeEventListener("keydown", handler, true);
  }, [config, toggle, setMute]);

  // Debug peak readout
  useEffect(() => {
    setDebugMsg(`peak: ${peakLevel.toFixed(6)}`);
  }, [peakLevel]);

  if (!config) return null;

  return (
    <>
      <Header
        isMuted={isMuted}
        peakLevel={peakLevel}
        onPeakUpdate={setPeakLevel}
        onToggleMute={toggle}
        config={config}
      />
      <TabNav activeTab={activeTab} onTabChange={setActiveTab} />
      <main className="settings-content">
        <div className={`tab-pane ${activeTab === "devices" ? "active" : ""}`}>
          <DevicesTab config={config} updateConfig={updateConfig} />
        </div>
        <div className={`tab-pane ${activeTab === "audio" ? "active" : ""}`}>
          <AudioTab config={config} updateConfig={updateConfig} />
        </div>
        <div className={`tab-pane ${activeTab === "hotkeys" ? "active" : ""}`}>
          <HotkeysTab config={config} updateConfig={updateConfig} />
        </div>
        <div className={`tab-pane ${activeTab === "overlay" ? "active" : ""}`}>
          <OverlayTab config={config} updateConfig={updateConfig} />
        </div>
        <div className={`tab-pane ${activeTab === "osd" ? "active" : ""}`}>
          <OsdTab config={config} updateConfig={updateConfig} />
        </div>
        <div className={`tab-pane ${activeTab === "system" ? "active" : ""}`}>
          <SystemTab config={config} updateConfig={updateConfig} />
        </div>
      </main>
      <Footer debugMsg={debugMsg} config={config} />
    </>
  );
}
