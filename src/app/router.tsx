import {
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  redirect,
} from "@tanstack/react-router";
import { BookshelfPage } from "@/features/bookshelf/bookshelf-page";
import { ProjectLayout } from "@/features/project/project-layout";
import { WritePage } from "@/features/chapters/write-page";
import { OutlinePage } from "@/features/outline/outline-page";
import { CharactersPage } from "@/features/characters/characters-page";
import { WorldPage } from "@/features/worldbuilding/world-page";
import { TimelinePage } from "@/features/timeline/timeline-page";
import { ForeshadowingPage } from "@/features/foreshadowing/foreshadowing-page";
import { StatsPage } from "@/features/stats/stats-page";
import { SettingsPage } from "@/features/settings/settings-page";

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
