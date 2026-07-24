import { invokeCmd } from "./client";
import type { AiConfigPublic, AiMode, ExportFormat } from "./types";

export function aiConfigSet(input: {
  baseUrl: string;
  apiKey: string;
  model: string;
}): Promise<void> {
  return invokeCmd("ai_config_set", input);
}

export function aiConfigGet(): Promise<AiConfigPublic> {
  return invokeCmd("ai_config_get", {});
}

export function aiComplete(input: {
  mode: AiMode;
  prompt: string;
  context?: string;
  stream?: boolean;
  requestId?: string;
}): Promise<{ finishReason: string }> {
  return invokeCmd("ai_complete", input);
}

export function aiCancel(requestId: string): Promise<void> {
  return invokeCmd("ai_cancel", { requestId });
}

export function exportRun(input: {
  projectId: string;
  format: ExportFormat;
  scope: {
    kind: "all" | "volume" | "chapters";
    volumeId?: string;
    chapterIds?: string[];
  };
  targetPath: string;
}): Promise<{ path: string }> {
  return invokeCmd("export_run", input);
}
