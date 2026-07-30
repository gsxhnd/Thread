import type { MouseEvent, ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauri } from "@/lib/platform";
import { cn } from "@/lib/utils";

type WindowTitleBarProps = {
  children?: ReactNode;
  className?: string;
  /** Extra content on the right (interactive; excluded from drag). */
  trailing?: ReactNode;
};

const NO_DRAG_SELECTOR =
  "a,button,input,textarea,select,[role='button'],.window-title-bar-interactive,[data-no-drag]";

function isNoDragTarget(target: EventTarget | null): boolean {
  return target instanceof Element && !!target.closest(NO_DRAG_SELECTOR);
}

/**
 * Overlay titlebar drag region for Tauri (`titleBarStyle: "Overlay"`).
 * CSS `-webkit-app-region` alone does not work — need `data-tauri-drag-region`
 * and/or `getCurrentWindow().startDragging()`.
 */
export function WindowTitleBar({
  children,
  className,
  trailing,
}: WindowTitleBarProps) {
  const onMouseDown = (e: MouseEvent<HTMLElement>) => {
    if (e.button !== 0 || isNoDragTarget(e.target) || !isTauri()) return;

    try {
      const win = getCurrentWindow();
      if (e.detail === 2) {
        void win.toggleMaximize();
        return;
      }
      void win.startDragging();
    } catch {
      // Browser / non-Tauri preview: ignore.
    }
  };

  return (
    <header
      onMouseDown={onMouseDown}
      className={cn(
        "window-title-bar relative flex h-10 shrink-0 items-center gap-2 border-b border-border bg-shell-primary px-3 select-none",
        "[padding-left:max(0.75rem,var(--titlebar-macos-inset))]",
        className,
      )}
    >
      {/* Full-bleed drag hit target (Tauri requires the attribute on the clicked node). */}
      <div data-tauri-drag-region className="absolute inset-0 z-0" aria-hidden />
      <div className="pointer-events-none relative z-[1] flex min-w-0 flex-1 items-center gap-2">
        {children}
      </div>
      {trailing ? (
        <div
          data-no-drag
          className="window-title-bar-interactive relative z-[1] flex shrink-0 items-center gap-1"
        >
          {trailing}
        </div>
      ) : null}
    </header>
  );
}
