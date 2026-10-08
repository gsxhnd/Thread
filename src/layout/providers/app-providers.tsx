import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ThemeProvider as NextThemesProvider } from "next-themes";
import { useEffect, type ReactNode } from "react";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Toaster } from "@/components/ui/sonner";
import { getPlatform, isTauri } from "@/lib/platform";
import { ensureVariableCompletion } from "@/lib/editor/monaco";
import { collectVariableSuggestions } from "@/lib/variables";
import { useAppStore } from "@/stores/app-store";
import { useUiStore } from "@/stores/ui-store";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 5_000,
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
});

function applyWindowChrome() {
  const platform = getPlatform();
  document.documentElement.dataset.platform = platform;
  if (isTauri() && platform === "macos") {
    document.documentElement.dataset.windowChrome = "macos-tauri";
  } else {
    delete document.documentElement.dataset.windowChrome;
  }
}

type AppProvidersProps = {
  children: ReactNode;
};

export function AppProviders({ children }: AppProvidersProps) {
  useEffect(() => {
    applyWindowChrome();
    void useUiStore.getState().hydrate();
    // Rust 侧启动时会尝试恢复上次的工作区，这里把状态同步到前端。
    void useAppStore.getState().bootstrap();
    // Monaco Body 编辑器中的 {{变量}} 补全。
    ensureVariableCompletion(() => collectVariableSuggestions(useAppStore.getState().snapshot));
  }, []);

  return (
    <QueryClientProvider client={queryClient}>
      <NextThemesProvider
        attribute="class"
        defaultTheme="system"
        enableSystem
        disableTransitionOnChange
      >
        <TooltipProvider delay={300}>
          {children}
          <Toaster position="bottom-right" />
        </TooltipProvider>
      </NextThemesProvider>
    </QueryClientProvider>
  );
}

export { queryClient };
