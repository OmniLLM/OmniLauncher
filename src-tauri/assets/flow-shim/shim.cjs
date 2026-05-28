#!/usr/bin/env node
// OmniLauncher entry shim for Flow.Launcher plugins.
//
// Auto-generated on install. Translates between OmniLauncher's stdin/stdout
// JSON protocol and Flow.Launcher's JSON-RPC-over-argv protocol.

"use strict";

const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const extDir = __dirname;
const flowManifestPath = path.join(extDir, "flow.plugin.json");

function readManifest() {
  try {
    return JSON.parse(fs.readFileSync(flowManifestPath, "utf8"));
  } catch (e) {
    process.stderr.write(`flow-shim: failed to read flow.plugin.json: ${e.message}\n`);
    return null;
  }
}

const manifest = readManifest() || {};
const language = String(manifest.Language || manifest.language || "").toLowerCase();
const executeFile = manifest.ExecuteFileName || manifest.executeFileName || "";
const actionKeyword = String(manifest.ActionKeyword || manifest.actionKeyword || "*");
const pluginName = manifest.Name || manifest.name || path.basename(extDir);
const pluginIcon = manifest.IcoPath || manifest.icoPath || "";

function reply(obj) {
  process.stdout.write(JSON.stringify(obj) + "\n");
}

// ─── Interpreter resolution ──────────────────────────────────────────────────

function pythonExecutable() {
  // Prefer OmniLauncher's bundled Python if present
  const home = process.env.USERPROFILE || process.env.HOME || "";
  const bundled =
    process.platform === "win32"
      ? path.join(home, ".omnilauncher", "python", "python.exe")
      : path.join(home, ".omnilauncher", "python", "bin", "python3");
  if (home && fs.existsSync(bundled)) return bundled;
  return process.platform === "win32" ? "python.exe" : "python3";
}

function nodeExecutable() {
  return process.execPath; // run with same node
}

function resolveSpawn(executePath, requestJson) {
  const abs = path.isAbsolute(executePath)
    ? executePath
    : path.join(extDir, executePath);

  switch (language) {
    case "python":
      return { cmd: pythonExecutable(), args: [abs, requestJson] };
    case "javascript":
      return { cmd: nodeExecutable(), args: [abs, requestJson] };
    case "typescript": {
      // Use npx tsx — best-effort. If unavailable, instruct user.
      const runner = process.platform === "win32" ? "npx.cmd" : "npx";
      return {
        cmd: runner,
        args: ["--no-install", "tsx", abs, requestJson],
        cwd: extDir,
      };
    }
    case "executable":
      return { cmd: abs, args: [requestJson] };
    case "csharp":
    case "fsharp":
      return null; // unsupported
    default:
      // Best-effort: infer from extension
      if (abs.toLowerCase().endsWith(".py"))
        return { cmd: pythonExecutable(), args: [abs, requestJson] };
      if (abs.toLowerCase().endsWith(".js"))
        return { cmd: nodeExecutable(), args: [abs, requestJson] };
      return { cmd: abs, args: [requestJson] };
  }
}

// ─── Flow.Launcher JSON-RPC bridge ───────────────────────────────────────────

function callFlowPlugin(rpcRequest, timeoutMs) {
  if (!executeFile) {
    return { error: "flow.plugin.json missing ExecuteFileName" };
  }
  if (language === "csharp" || language === "fsharp") {
    return {
      error:
        `Flow.Launcher C#/F# plugin '${pluginName}' is not supported by ` +
        `OmniLauncher (no .NET host).`,
    };
  }

  const spawn = resolveSpawn(executeFile, JSON.stringify(rpcRequest));
  if (!spawn) {
    return { error: `Unsupported Flow.Launcher language: '${language}'` };
  }

  const result = spawnSync(spawn.cmd, spawn.args, {
    cwd: spawn.cwd || extDir,
    timeout: timeoutMs,
    encoding: "utf8",
    env: {
      ...process.env,
      PYTHONIOENCODING: "utf-8",
      PYTHONUTF8: "1",
    },
  });

  if (result.error) {
    return { error: `Failed to spawn '${spawn.cmd}': ${result.error.message}` };
  }
  if (result.status !== 0 && !(result.stdout || "").trim()) {
    return {
      error:
        `Flow plugin exited with code ${result.status}. ` +
        `stderr: ${(result.stderr || "").trim().slice(0, 400)}`,
    };
  }

  const stdout = (result.stdout || "").trim();
  if (!stdout) return { value: { result: [] } };

  // Some plugins emit log lines before JSON; find the JSON object/array.
  const firstBrace = Math.min(
    ...["{", "["]
      .map((c) => {
        const i = stdout.indexOf(c);
        return i < 0 ? Number.MAX_SAFE_INTEGER : i;
      })
  );
  const jsonText =
    firstBrace !== Number.MAX_SAFE_INTEGER ? stdout.slice(firstBrace) : stdout;

  try {
    return { value: JSON.parse(jsonText) };
  } catch (e) {
    return {
      error: `Flow plugin returned non-JSON output: ${e.message}`,
    };
  }
}

function pickField(obj, ...names) {
  for (const n of names) {
    if (obj && obj[n] != null) return obj[n];
  }
  return undefined;
}

function resolveIcon(icoPath) {
  if (!icoPath) return pluginIcon || undefined;
  if (path.isAbsolute(icoPath)) return icoPath;
  // Relative — try resolving against plugin dir; fall back to literal.
  const abs = path.join(extDir, icoPath);
  return fs.existsSync(abs) ? abs : icoPath;
}

function translateResults(flowResp) {
  const arr =
    pickField(flowResp, "result", "Result", "results", "Results") || [];
  if (!Array.isArray(arr)) return [];

  return arr.map((item, i) => {
    const title = pickField(item, "Title", "title") || "";
    const subtitle = pickField(item, "SubTitle", "subTitle", "subtitle");
    const icoPath = pickField(item, "IcoPath", "icoPath", "IconPath");
    const score = pickField(item, "Score", "score");
    const action =
      pickField(item, "JsonRPCAction", "jsonRPCAction", "Action", "action") ||
      null;

    // Encode the Flow JsonRPCAction as our action_data so we can route it back.
    const actionPayload = action
      ? JSON.stringify(action)
      : JSON.stringify({ method: "", parameters: [] });

    return {
      id: `${pluginName}::${i}`,
      title: String(title),
      subtitle: subtitle != null ? String(subtitle) : undefined,
      icon: resolveIcon(icoPath),
      score: typeof score === "number" ? score : 50,
      action_type: "plugin_execute",
      action_data: actionPayload,
    };
  });
}

function stripActionKeyword(query) {
  if (!actionKeyword || actionKeyword === "*") return query || "";
  const trimmed = (query || "").trim();
  if (trimmed.toLowerCase().startsWith(actionKeyword.toLowerCase())) {
    return trimmed.slice(actionKeyword.length).trimStart();
  }
  return trimmed;
}

// ─── Main loop ───────────────────────────────────────────────────────────────

function handleQuery(rawQuery) {
  const stripped = stripActionKeyword(rawQuery);
  const rpc = { method: "query", parameters: [stripped] };
  const out = callFlowPlugin(rpc, 3500);
  if (out.error) {
    process.stderr.write(`flow-shim: ${out.error}\n`);
    reply({ results: [] });
    return;
  }
  reply({ results: translateResults(out.value) });
}

function handleExecute(actionData) {
  // actionData is JSON of the original JsonRPCAction
  let action;
  try {
    action = JSON.parse(actionData);
  } catch {
    action = { method: "", parameters: [] };
  }
  const method = action.method || action.Method || "";
  const parameters = action.parameters || action.Parameters || [];

  if (!method) {
    reply({ output: "No Flow action attached to result." });
    return;
  }

  const out = callFlowPlugin({ method, parameters }, 10000);
  if (out.error) {
    reply({ output: out.error });
    return;
  }

  // The Flow plugin may emit Flow.Launcher.* API calls in its response;
  // we honor a subset (open URL, copy, run shell).
  const response = out.value || {};
  let handled = false;
  let summary = "";

  const settingsBlock =
    response.SettingsChange || response.settingsChange || null;
  if (settingsBlock) {
    summary += `[settings] updated ${Object.keys(settingsBlock).length} keys\n`;
  }

  // Process embedded API calls if any (some plugins return both result+actions)
  const actions =
    response.DebugMessage ||
    response.debugMessage ||
    response.method ||
    response.Method;
  if (typeof actions === "string" && actions) {
    summary += `${actions}\n`;
  }

  // Generic: copy stdout summary
  if (!handled) {
    summary = summary.trim() || `Ran ${pluginName}::${method}`;
  }
  reply({ output: summary });
}

// Invoked by the AI agent via `op: tool_call`. Runs a Flow `query`
// JSON-RPC call with the supplied text, summarizes the top results,
// and (if the top result has an attached action) executes it so the
// agent gets the actual output.
function handleToolCall(args) {
  const a = args && typeof args === "object" ? args : {};
  const query = typeof a.query === "string" ? a.query : "";

  const rpc = { method: "query", parameters: [query] };
  const out = callFlowPlugin(rpc, 5000);
  if (out.error) {
    reply({ output: `Flow plugin '${pluginName}' query failed: ${out.error}` });
    return;
  }

  const arr =
    pickField(out.value || {}, "result", "Result", "results", "Results") || [];
  if (!Array.isArray(arr) || arr.length === 0) {
    reply({ output: `Flow plugin '${pluginName}' returned no results for "${query}".` });
    return;
  }

  const top = arr.slice(0, 5).map((item, i) => {
    const title = pickField(item, "Title", "title") || "";
    const subtitle = pickField(item, "SubTitle", "subTitle", "subtitle") || "";
    return subtitle
      ? `${i + 1}. ${title} — ${subtitle}`
      : `${i + 1}. ${title}`;
  });

  // If the first result has a JsonRPCAction, run it so the agent sees the
  // side-effect output too (e.g. a converted value, a calc result).
  const firstAction =
    pickField(arr[0], "JsonRPCAction", "jsonRPCAction", "Action", "action") ||
    null;
  let actionOutput = "";
  if (firstAction && (firstAction.method || firstAction.Method)) {
    const exec = callFlowPlugin(
      {
        method: firstAction.method || firstAction.Method,
        parameters: firstAction.parameters || firstAction.Parameters || [],
      },
      10000
    );
    if (exec.value) {
      const dbg =
        pickField(exec.value, "DebugMessage", "debugMessage") || "";
      if (typeof dbg === "string" && dbg) actionOutput = `\n[action] ${dbg}`;
    } else if (exec.error) {
      actionOutput = `\n[action error] ${exec.error}`;
    }
  }

  reply({
    output:
      `Flow plugin '${pluginName}' results for "${query}":\n` +
      top.join("\n") +
      actionOutput,
  });
}

(async () => {
  let raw = "";
  process.stdin.setEncoding("utf8");
  process.stdin.on("data", (d) => (raw += d));
  process.stdin.on("end", () => {
    let req = {};
    try {
      req = JSON.parse(raw);
    } catch {
      /* ignore */
    }
    if (req.op === "query") {
      handleQuery(req.query || "");
    } else if (req.op === "execute") {
      handleExecute(req.action_data || "");
    } else if (req.op === "tool_call") {
      handleToolCall(req.args || {});
    } else {
      reply({ results: [] });
    }
  });
})();
