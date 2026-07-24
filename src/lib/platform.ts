/**
 * Runtime platform detection for Tauri vs browser and OS family.
 *
 * 运行时平台检测：判断当前是否在 Tauri 桌面环境中运行，以及操作系统族
 * （macOS / Windows / Linux），用于快捷键显示、窗口装饰与功能降级。
 */
export type Platform = "macos" | "windows" | "linux";

export function isTauri(): boolean {
  return (
    typeof window !== "undefined" &&
    ("__TAURI_INTERNALS__" in window || "__TAURI__" in window)
  );
}

export function getPlatform(): Platform {
  const envPlatform = import.meta.env.TAURI_ENV_PLATFORM as string | undefined;
  if (envPlatform === "macos" || envPlatform === "darwin") return "macos";
  if (envPlatform === "windows") return "windows";
  if (envPlatform === "linux") return "linux";

  if (typeof navigator !== "undefined") {
    const ua = navigator.userAgent.toLowerCase();
    if (ua.includes("mac")) return "macos";
    if (ua.includes("win")) return "windows";
  }

  return "linux";
}
