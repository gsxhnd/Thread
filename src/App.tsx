import { AppShell } from "@/layout/app-shell";
import { ConnectionsShell } from "@/layout/connections-shell";
import { EditorShell } from "@/layout/editor-shell";
import { SettingsShell } from "@/layout/settings-shell";
import { AppProviders } from "@/layout/providers/app-providers";
import { getAppWindowMode } from "@/lib/app-window";
import "./App.css";

function App() {
  const mode = getAppWindowMode();

  return (
    <AppProviders>
      {mode === "settings" ? (
        <SettingsShell />
      ) : mode === "connections" ? (
        <ConnectionsShell />
      ) : mode === "editor" ? (
        <EditorShell />
      ) : (
        <AppShell />
      )}
    </AppProviders>
  );
}

export default App;
