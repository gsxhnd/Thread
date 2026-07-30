import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ThemeProvider as NextThemesProvider } from "next-themes";
import { useEffect, type ReactNode } from "react";
import { I18nextProvider } from "react-i18next";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Toaster } from "@/components/ui/sonner";
import i18n from "@/i18n";
import { getPlatform, isTauri } from "@/lib/platform";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
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
  document.querySelectorAll<HTMLElement>("[data-app-shell]").forEach((el) => {
    el.dataset.platform = platform;
  });
}

type AppProvidersProps = {
  children: ReactNode;
};

export function AppProviders({ children }: AppProvidersProps) {
  useEffect(() => {
    applyWindowChrome();
    const observer = new MutationObserver(() => applyWindowChrome());
    observer.observe(document.body, { childList: true, subtree: true });
    return () => observer.disconnect();
  }, []);

  return (
    <I18nextProvider i18n={i18n}>
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
    </I18nextProvider>
  );
}

export { queryClient };
