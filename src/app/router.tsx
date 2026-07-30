import {
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  redirect,
} from "@tanstack/react-router";
import { ProjectLayout } from "@/layout/project-layout";
import { BookshelfPage } from "@/pages/bookshelf-page";
import { WritePage } from "@/pages/write-page";
import { OutlinePage } from "@/pages/outline-page";
import { CharactersPage } from "@/pages/characters-page";
import { WorldPage } from "@/pages/world-page";
import { TimelinePage } from "@/pages/timeline-page";
import { ForeshadowingPage } from "@/pages/foreshadowing-page";
import { StatsPage } from "@/pages/stats-page";
import { SettingsPage } from "@/pages/settings-page";

const rootRoute = createRootRoute({
  component: () => <Outlet />,
});

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: BookshelfPage,
});

const projectRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/project/$projectId",
  component: ProjectLayout,
});

const projectIndexRoute = createRoute({
  getParentRoute: () => projectRoute,
  path: "/",
  beforeLoad: ({ params }) => {
    throw redirect({
      to: "/project/$projectId/write",
      params: { projectId: params.projectId },
    });
  },
});

const writeRoute = createRoute({
  getParentRoute: () => projectRoute,
  path: "/write",
  component: WritePage,
});

const writeChapterRoute = createRoute({
  getParentRoute: () => projectRoute,
  path: "/write/$chapterId",
  component: WritePage,
});

const outlineRoute = createRoute({
  getParentRoute: () => projectRoute,
  path: "/outline",
  component: OutlinePage,
});

const charactersRoute = createRoute({
  getParentRoute: () => projectRoute,
  path: "/characters",
  component: CharactersPage,
});

const worldRoute = createRoute({
  getParentRoute: () => projectRoute,
  path: "/world",
  component: WorldPage,
});

const timelineRoute = createRoute({
  getParentRoute: () => projectRoute,
  path: "/timeline",
  component: TimelinePage,
});

const foreshadowingRoute = createRoute({
  getParentRoute: () => projectRoute,
  path: "/foreshadowing",
  component: ForeshadowingPage,
});

const statsRoute = createRoute({
  getParentRoute: () => projectRoute,
  path: "/stats",
  component: StatsPage,
});

const settingsRoute = createRoute({
  getParentRoute: () => projectRoute,
  path: "/settings",
  component: SettingsPage,
});

const routeTree = rootRoute.addChildren([
  indexRoute,
  projectRoute.addChildren([
    projectIndexRoute,
    writeRoute,
    writeChapterRoute,
    outlineRoute,
    charactersRoute,
    worldRoute,
    timelineRoute,
    foreshadowingRoute,
    statsRoute,
    settingsRoute,
  ]),
]);

export const router = createRouter({
  routeTree,
  defaultPreload: "intent",
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
