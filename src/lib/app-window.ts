export type AppWindowMode = "main" | "settings" | "connections" | "editor";

export const SETTINGS_WINDOW_LABEL = "settings";
export const CONNECTIONS_WINDOW_LABEL = "connections";

export function getAppWindowMode(): AppWindowMode {
  const params = new URLSearchParams(window.location.search);
  const mode = params.get("window");
  if (mode === "settings") return "settings";
  if (mode === "connections") return "connections";
  if (mode === "editor") return "editor";
  return "main";
}

export function buildEditorWindowUrl(args: {
  path: string;
  source: "local" | "remote";
  sessionId?: string;
}): string {
  const url = new URL(window.location.href);
  url.searchParams.set("window", "editor");
  url.searchParams.set("path", args.path);
  url.searchParams.set("source", args.source);
  if (args.sessionId) {
    url.searchParams.set("sessionId", args.sessionId);
  } else {
    url.searchParams.delete("sessionId");
  }
  return url.toString();
}

export function buildSettingsWindowUrl(): string {
  const url = new URL(window.location.href);
  url.searchParams.set("window", "settings");
  return url.toString();
}

export function buildConnectionsWindowUrl(): string {
  const url = new URL(window.location.href);
  url.searchParams.set("window", "connections");
  return url.toString();
}
