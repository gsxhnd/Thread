import { describe, expect, it } from "vitest";
import type { EnvironmentSummary, WorkspaceSnapshot } from "@/lib/api";
import {
  applyVariableCompletion,
  filterSuggestions,
  collectVariableSuggestions,
  findVariableQuery,
  insertVariablePlaceholder,
} from "@/lib/variables";

function environment(
  name: string,
  variableNames: string[],
  options: { secret?: string[]; isGlobal?: boolean; isActive?: boolean } = {},
): EnvironmentSummary {
  return {
    id: name,
    name,
    path: `environments/${name}.env.json`,
    variableCount: variableNames.length,
    secretCount: options.secret?.length ?? 0,
    variableNames,
    secretVariableNames: options.secret ?? [],
    isGlobal: options.isGlobal ?? false,
    isActive: options.isActive ?? false,
  };
}

function snapshot(environments: EnvironmentSummary[]): WorkspaceSnapshot {
  return {
    index: {
      id: "ws",
      name: "WS",
      version: 1,
      root: "/tmp/ws",
      activeEnvironment: null,
      collections: [],
      environments,
      collectionCount: 0,
      requestCount: 0,
      warnings: [],
      scannedAt: 0,
      digest: "0",
    },
    config: {
      version: 1,
      name: "WS",
      proxy: { port: 9090, autoStart: false },
      settings: {},
    },
    warnings: [],
  };
}

describe("collectVariableSuggestions", () => {
  it("orders active environment first, then global, then others", () => {
    const suggestions = collectVariableSuggestions(
      snapshot([
        environment("dev", ["baseUrl", "apiKey"], {
          secret: ["apiKey"],
          isActive: true,
        }),
        environment("global", ["sharedToken"], { isGlobal: true }),
        environment("prod", ["host"]),
      ]),
    );

    expect(suggestions.slice(0, 3).map((item) => item.key)).toEqual([
      "baseUrl",
      "apiKey",
      "sharedToken",
    ]);
    expect(suggestions.find((item) => item.key === "apiKey")?.secret).toBe(true);
    expect(suggestions.find((item) => item.key === "baseUrl")?.active).toBe(true);
    expect(suggestions.find((item) => item.key === "host")?.active).toBe(false);
  });

  it("deduplicates keys across environments and appends builtins", () => {
    const suggestions = collectVariableSuggestions(
      snapshot([
        environment("dev", ["baseUrl"], { isActive: true }),
        environment("prod", ["baseUrl", "host"]),
      ]),
    );
    expect(suggestions.filter((item) => item.key === "baseUrl")).toHaveLength(1);
    expect(suggestions.map((item) => item.key)).toEqual(
      expect.arrayContaining(["baseUrl", "host", "$uuid", "$timestamp"]),
    );
    expect(suggestions.find((item) => item.key === "$uuid")?.builtin).toBe(true);
  });

  it("handles a missing workspace", () => {
    const suggestions = collectVariableSuggestions(null);
    expect(suggestions.map((item) => item.key)).toEqual([
      "$uuid",
      "$timestamp",
      "$isoTimestamp",
      "$randomInt",
    ]);
  });
});

describe("findVariableQuery", () => {
  it("detects an open placeholder before the caret", () => {
    expect(findVariableQuery("{{ba", 4)).toEqual({ start: 0, query: "ba" });
    expect(findVariableQuery("https://x/{{ba", 14)).toEqual({
      start: 10,
      query: "ba",
    });
    expect(findVariableQuery("{{", 2)).toEqual({ start: 0, query: "" });
  });

  it("ignores closed placeholders, whitespace and nested braces", () => {
    expect(findVariableQuery("{{baseUrl}}/users", 17)).toBeNull();
    expect(findVariableQuery("{{base Url", 11)).toBeNull();
    expect(findVariableQuery("{{a{b", 5)).toBeNull();
    expect(findVariableQuery("{{a}b", 5)).toBeNull();
    expect(findVariableQuery("plain text", 10)).toBeNull();
  });

  it("completes against the innermost open placeholder", () => {
    expect(findVariableQuery("{{ {{a", 6)).toEqual({ start: 3, query: "a" });
  });
});

describe("applyVariableCompletion", () => {
  it("replaces the partial placeholder and closes the braces", () => {
    const active = findVariableQuery("https://{{ba", 12);
    expect(active).not.toBeNull();
    const result = applyVariableCompletion("https://{{ba", 12, active!, "baseUrl");
    expect(result.value).toBe("https://{{baseUrl}}");
    expect(result.caret).toBe(result.value.length);
  });

  it("keeps the text after the caret", () => {
    const source = "{{ba}}/users";
    const caret = 4;
    const active = findVariableQuery(source, caret);
    expect(active).not.toBeNull();
    const result = applyVariableCompletion(source, caret, active!, "baseUrl");
    // 光标后已存在的 `}}` 会被吃掉，避免生成 `{{baseUrl}}}}`。
    expect(result.value).toBe("{{baseUrl}}/users");
    expect(result.caret).toBe(11);
  });
});

describe("insertVariablePlaceholder", () => {
  it("inserts at the caret without a pre-existing placeholder", () => {
    expect(insertVariablePlaceholder("", 0, 0, "baseUrl")).toEqual({
      value: "{{baseUrl}}",
      caret: 11,
    });
    expect(insertVariablePlaceholder("users", 0, 0, "baseUrl")).toEqual({
      value: "{{baseUrl}}users",
      caret: 11,
    });
  });

  it("replaces the current selection", () => {
    const result = insertVariablePlaceholder("https://staging.example.com", 8, 15, "host");
    expect(result.value).toBe("https://{{host}}.example.com");
    expect(result.caret).toBe(16);
  });

  it("swallows an existing closing brace pair", () => {
    const result = insertVariablePlaceholder("{{}}/users", 2, 2, "baseUrl");
    expect(result.value).toBe("{{baseUrl}}/users");
  });

  it("appends at the end for out-of-range carets", () => {
    expect(insertVariablePlaceholder("abc", 99, 99, "k").value).toBe("abc{{k}}");
  });
});

describe("filterSuggestions", () => {
  const suggestions = collectVariableSuggestions(
    snapshot([environment("dev", ["baseUrl", "apiKey", "host"], { isActive: true })]),
  );

  it("filters by substring and prefers prefix matches", () => {
    expect(filterSuggestions(suggestions, "key").map((item) => item.key)).toEqual([
      "apiKey",
    ]);
    expect(filterSuggestions(suggestions, "ba").map((item) => item.key)).toContain(
      "baseUrl",
    );
    expect(filterSuggestions(suggestions, "$").map((item) => item.key)).toEqual([
      "$isoTimestamp",
      "$randomInt",
      "$timestamp",
      "$uuid",
    ]);
  });

  it("limits the number of rows and returns everything for an empty query", () => {
    expect(filterSuggestions(suggestions, "", 2)).toHaveLength(2);
    expect(filterSuggestions(suggestions, "nope")).toEqual([]);
  });
});
