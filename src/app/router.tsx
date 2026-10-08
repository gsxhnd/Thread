import {
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  redirect,
} from "@tanstack/react-router";
import { WelcomePage } from "@/pages/welcome-page";
import { WorkspacePage } from "@/pages/workspace-page";
import { useAppStore } from "@/stores/app-store";

const rootRoute = createRootRoute({
  component: () => <Outlet />,
});

const welcomeRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: WelcomePage,
});

const workspaceRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/workspace",
  beforeLoad: () => {
    if (!useAppStore.getState().snapshot) {
      throw redirect({ to: "/" });
    }
  },
  component: WorkspacePage,
});

const routeTree = rootRoute.addChildren([welcomeRoute, workspaceRoute]);

export const router = createRouter({
  routeTree,
  defaultPreload: false,
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
