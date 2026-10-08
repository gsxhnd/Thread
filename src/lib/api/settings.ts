/** 应用设置（与工作区无关，保存在 ~/.config/thread/config.toml）。 */
import { invokeCmd } from "./client";
import type { AppSettings } from "./types";

export function appSettingsGet(): Promise<AppSettings> {
  return invokeCmd("app_settings_get");
}

export function appSettingsSet(settings: AppSettings): Promise<AppSettings> {
  return invokeCmd("app_settings_set", { settings });
}

export function setConfigSection(section: string, value: string): Promise<void> {
  return invokeCmd("set_config_section", { section, value });
}

export function loadConfigSections(): Promise<Record<string, string>> {
  return invokeCmd("load_config_sections");
}

export function getConfigLoadWarnings(): Promise<string[]> {
  return invokeCmd("get_config_load_warnings");
}
