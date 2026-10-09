import { create } from "zustand";
import type { GraphqlSchema, ProtoServiceInfo } from "@/lib/api";

export type LiveStatus = "idle" | "connecting" | "open" | "closed" | "error";

export type LiveMessage = {
  id: string;
  direction: "in" | "out" | "status";
  text: string;
  at: number;
};

export type LiveSession = {
  connectionId?: string;
  status: LiveStatus;
  messages: LiveMessage[];
  services: ProtoServiceInfo[];
  poolId?: string;
  schema?: GraphqlSchema | null;
};

const MESSAGE_LIMIT = 400;

function emptySession(): LiveSession {
  return { status: "idle", messages: [], services: [] };
}

function withMessage(session: LiveSession, message: LiveMessage): LiveSession {
  const messages = [...session.messages, message];
  return {
    ...session,
    messages: messages.length > MESSAGE_LIMIT ? messages.slice(messages.length - MESSAGE_LIMIT) : messages,
  };
}

type PendingEvent = {
  messages: Omit<LiveMessage, "id">[];
  status?: LiveStatus;
  reason?: string;
};

interface SessionState {
  byTab: Record<string, LiveSession>;
  tabByConnection: Record<string, string>;
  pending: Record<string, PendingEvent>;
  session: (tabKey: string) => LiveSession;
  patch: (tabKey: string, patch: Partial<LiveSession>) => void;
  bind: (tabKey: string, connectionId: string) => void;
  pushByConnection: (
    connectionId: string,
    message: Omit<LiveMessage, "id">,
  ) => void;
  pushByTab: (tabKey: string, message: Omit<LiveMessage, "id">) => void;
  setStatus: (connectionId: string, status: LiveStatus, reason?: string) => void;
  rekey: (from: string, to: string) => void;
  release: (tabKey: string) => void;
  forget: (tabKey: string) => void;
  connectionIds: () => { tabKey: string; protocolHint?: string; connectionId: string }[];
}

let messageSeq = 0;

function nextMessage(input: Omit<LiveMessage, "id">): LiveMessage {
  messageSeq += 1;
  return { ...input, id: `${input.at}-${messageSeq}` };
}

function queuePending(
  pending: Record<string, PendingEvent>,
  connectionId: string,
  update: (current: PendingEvent) => PendingEvent,
): Record<string, PendingEvent> {
  const current = pending[connectionId] ?? { messages: [] };
  return { ...pending, [connectionId]: update(current) };
}

export const useSessionStore = create<SessionState>((set, get) => ({
  byTab: {},
  tabByConnection: {},
  pending: {},

  session: (tabKey) => get().byTab[tabKey] ?? emptySession(),

  patch: (tabKey, patch) =>
    set((state) => ({
      byTab: {
        ...state.byTab,
        [tabKey]: { ...(state.byTab[tabKey] ?? emptySession()), ...patch },
      },
    })),

  bind: (tabKey, connectionId) =>
    set((state) => {
      const previous = state.byTab[tabKey]?.connectionId;
      const tabByConnection = { ...state.tabByConnection };
      if (previous && previous !== connectionId) delete tabByConnection[previous];
      tabByConnection[connectionId] = tabKey;
      const queued = state.pending[connectionId];
      const pending = { ...state.pending };
      delete pending[connectionId];
      let session: LiveSession = {
        ...(state.byTab[tabKey] ?? emptySession()),
        connectionId,
        status: queued?.status ?? "connecting",
        messages: [],
      };
      for (const message of queued?.messages ?? []) {
        session = withMessage(session, nextMessage(message));
      }
      if (queued?.reason) {
        session = withMessage(
          session,
          nextMessage({ direction: "status", text: queued.reason, at: Date.now() }),
        );
      }
      return {
        tabByConnection,
        pending,
        byTab: { ...state.byTab, [tabKey]: session },
      };
    }),

  pushByConnection: (connectionId, message) =>
    set((state) => {
      const tabKey = state.tabByConnection[connectionId];
      if (!tabKey) {
        return {
          pending: queuePending(state.pending, connectionId, (current) => ({
            ...current,
            messages: [...current.messages, message],
          })),
        };
      }
      const current = state.byTab[tabKey] ?? emptySession();
      return {
        byTab: {
          ...state.byTab,
          [tabKey]: withMessage(current, nextMessage(message)),
        },
      };
    }),

  pushByTab: (tabKey, message) =>
    set((state) => {
      const current = state.byTab[tabKey] ?? emptySession();
      return {
        byTab: {
          ...state.byTab,
          [tabKey]: withMessage(current, nextMessage(message)),
        },
      };
    }),

  setStatus: (connectionId, status, reason) =>
    set((state) => {
      const tabKey = state.tabByConnection[connectionId];
      if (!tabKey) {
        return {
          pending: queuePending(state.pending, connectionId, (current) => ({
            ...current,
            status,
            reason: reason ?? current.reason,
          })),
        };
      }
      const current = state.byTab[tabKey] ?? emptySession();
      const next = { ...current, status };
      return {
        byTab: {
          ...state.byTab,
          [tabKey]: reason
            ? withMessage(next, nextMessage({ direction: "status", text: reason, at: Date.now() }))
            : next,
        },
      };
    }),

  rekey: (from, to) =>
    set((state) => {
      if (from === to) return state;
      const session = state.byTab[from];
      if (!session) return state;
      const byTab = { ...state.byTab };
      delete byTab[from];
      byTab[to] = session;
      const tabByConnection = { ...state.tabByConnection };
      if (session.connectionId) tabByConnection[session.connectionId] = to;
      return { byTab, tabByConnection };
    }),

  release: (tabKey) =>
    set((state) => {
      const session = state.byTab[tabKey];
      if (!session) return state;
      const tabByConnection = { ...state.tabByConnection };
      const pending = { ...state.pending };
      if (session.connectionId) {
        delete tabByConnection[session.connectionId];
        delete pending[session.connectionId];
      }
      return {
        tabByConnection,
        pending,
        byTab: {
          ...state.byTab,
          [tabKey]: {
            ...session,
            connectionId: undefined,
            status: session.connectionId ? "closed" : session.status,
          },
        },
      };
    }),

  forget: (tabKey) =>
    set((state) => {
      const id = state.byTab[tabKey]?.connectionId;
      if (!state.byTab[tabKey]) return state;
      const byTab = { ...state.byTab };
      delete byTab[tabKey];
      const tabByConnection = { ...state.tabByConnection };
      const pending = { ...state.pending };
      if (id) {
        delete tabByConnection[id];
        delete pending[id];
      }
      return { byTab, tabByConnection, pending };
    }),

  connectionIds: () =>
    Object.entries(get().byTab).flatMap(([tabKey, session]) =>
      session.connectionId ? [{ tabKey, connectionId: session.connectionId }] : [],
    ),
}));
