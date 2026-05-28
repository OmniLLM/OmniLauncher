// Mock implementation of `@raycast/api` for OmniLauncher.
//
// This shim is intentionally a best-effort emulation: it captures side
// effects (toast, HUD, clipboard) as text output, performs OS-level
// clipboard / open actions where possible, and provides component stubs
// so React-based command modules can be required without crashing.
//
// Full UI rendering (List, Form, ActionPanel) is NOT supported — view
// commands degrade to returning captured text.

"use strict";

const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawnSync } = require("child_process");

let _captured = [];
function __resetCapture() {
  _captured = [];
}
function __getCapture() {
  return _captured.slice();
}

function showToast(opts) {
  const title = (opts && (opts.title || opts.message)) || "";
  if (title) _captured.push(`[toast] ${title}`);
  return Promise.resolve({ hide: () => {} });
}

function showHUD(text) {
  if (text) _captured.push(`[hud] ${text}`);
  return Promise.resolve();
}

function closeMainWindow() {
  return Promise.resolve();
}
function popToRoot() {
  return Promise.resolve();
}

function _osCopy(text) {
  try {
    if (process.platform === "win32") {
      spawnSync("powershell", ["-NoProfile", "-Command", "$input | Set-Clipboard"], {
        input: text,
      });
    } else if (process.platform === "darwin") {
      spawnSync("pbcopy", [], { input: text });
    } else {
      spawnSync("xclip", ["-selection", "clipboard"], { input: text });
    }
  } catch {
    /* best effort */
  }
}

function _osPasteText() {
  try {
    if (process.platform === "win32") {
      const r = spawnSync("powershell", ["-NoProfile", "-Command", "Get-Clipboard -Raw"]);
      return (r.stdout || "").toString();
    }
    if (process.platform === "darwin") {
      const r = spawnSync("pbpaste");
      return (r.stdout || "").toString();
    }
    const r = spawnSync("xclip", ["-selection", "clipboard", "-o"]);
    return (r.stdout || "").toString();
  } catch {
    return "";
  }
}

const Clipboard = {
  async copy(text) {
    const s =
      typeof text === "string"
        ? text
        : text && typeof text === "object" && "text" in text
        ? String(text.text)
        : JSON.stringify(text);
    _captured.push(`[clipboard] ${s.slice(0, 200)}`);
    _osCopy(s);
  },
  async paste(text) {
    return this.copy(text);
  },
  async read() {
    return { text: _osPasteText() };
  },
  async readText() {
    return _osPasteText();
  },
  async clear() {
    _osCopy("");
  },
};

async function open(target, _appOrOpts) {
  _captured.push(`[open] ${target}`);
  try {
    if (process.platform === "win32") {
      spawnSync("cmd", ["/c", "start", "", target], { detached: true });
    } else if (process.platform === "darwin") {
      spawnSync("open", [target]);
    } else {
      spawnSync("xdg-open", [target]);
    }
  } catch {
    /* ignore */
  }
}

const Toast = {
  Style: { Success: "success", Failure: "failure", Animated: "animated" },
};

function getPreferenceValues() {
  const extDir = process.env.OMNI_RAYCAST_EXT_DIR || "";
  const extName = extDir ? path.basename(extDir) : "default";
  const file = path.join(
    os.homedir(),
    ".omnilauncher",
    "raycast-prefs",
    `${extName}.json`
  );
  try {
    if (fs.existsSync(file)) {
      return JSON.parse(fs.readFileSync(file, "utf8"));
    }
  } catch {
    /* ignore */
  }
  return {};
}

function getSelectedText() {
  return Promise.resolve(process.env.OMNI_SELECTED_TEXT || "");
}
function getSelectedFinderItems() {
  return Promise.resolve([]);
}

const environment = {
  commandName: process.env.OMNI_RAYCAST_COMMAND || "",
  extensionName: process.env.OMNI_RAYCAST_EXT_DIR
    ? path.basename(process.env.OMNI_RAYCAST_EXT_DIR)
    : "",
  isDevelopment: false,
  launchType: "userInitiated",
  raycastVersion: "0.0.0-omnilauncher-shim",
  supportPath: path.join(os.homedir(), ".omnilauncher", "raycast-support"),
  assetsPath: process.env.OMNI_RAYCAST_EXT_DIR
    ? path.join(process.env.OMNI_RAYCAST_EXT_DIR, "assets")
    : "",
  textSize: "medium",
  theme: "dark",
  appearance: "dark",
  canAccess: () => false,
};

const _lsBaseDir = path.join(os.homedir(), ".omnilauncher", "raycast-storage");
function _lsFile() {
  fs.mkdirSync(_lsBaseDir, { recursive: true });
  const name = environment.extensionName || "default";
  return path.join(_lsBaseDir, `${name}.json`);
}
function _lsRead() {
  try {
    return JSON.parse(fs.readFileSync(_lsFile(), "utf8"));
  } catch {
    return {};
  }
}
function _lsWrite(obj) {
  fs.writeFileSync(_lsFile(), JSON.stringify(obj, null, 2));
}

const LocalStorage = {
  async getItem(key) {
    const v = _lsRead()[key];
    return v === undefined ? undefined : v;
  },
  async setItem(key, value) {
    const o = _lsRead();
    o[key] = value;
    _lsWrite(o);
  },
  async removeItem(key) {
    const o = _lsRead();
    delete o[key];
    _lsWrite(o);
  },
  async allItems() {
    return _lsRead();
  },
  async clear() {
    _lsWrite({});
  },
};

function makeComponent(name) {
  const C = function (props) {
    return { type: name, props };
  };
  C.displayName = name;
  return C;
}

const List = makeComponent("List");
List.Item = makeComponent("List.Item");
List.Section = makeComponent("List.Section");
List.EmptyView = makeComponent("List.EmptyView");
List.Dropdown = makeComponent("List.Dropdown");
List.Dropdown.Item = makeComponent("List.Dropdown.Item");
List.Dropdown.Section = makeComponent("List.Dropdown.Section");

const Detail = makeComponent("Detail");
Detail.Metadata = makeComponent("Detail.Metadata");
Detail.Metadata.Label = makeComponent("Detail.Metadata.Label");
Detail.Metadata.Link = makeComponent("Detail.Metadata.Link");
Detail.Metadata.TagList = makeComponent("Detail.Metadata.TagList");
Detail.Metadata.TagList.Item = makeComponent("Detail.Metadata.TagList.Item");
Detail.Metadata.Separator = makeComponent("Detail.Metadata.Separator");

const Form = makeComponent("Form");
Form.TextField = makeComponent("Form.TextField");
Form.TextArea = makeComponent("Form.TextArea");
Form.Checkbox = makeComponent("Form.Checkbox");
Form.Dropdown = makeComponent("Form.Dropdown");
Form.Dropdown.Item = makeComponent("Form.Dropdown.Item");
Form.PasswordField = makeComponent("Form.PasswordField");
Form.DatePicker = makeComponent("Form.DatePicker");
Form.Separator = makeComponent("Form.Separator");
Form.TagPicker = makeComponent("Form.TagPicker");
Form.TagPicker.Item = makeComponent("Form.TagPicker.Item");
Form.FilePicker = makeComponent("Form.FilePicker");

const Grid = makeComponent("Grid");
Grid.Item = makeComponent("Grid.Item");
Grid.Section = makeComponent("Grid.Section");
Grid.EmptyView = makeComponent("Grid.EmptyView");

const Action = makeComponent("Action");
Action.CopyToClipboard = makeComponent("Action.CopyToClipboard");
Action.OpenInBrowser = makeComponent("Action.OpenInBrowser");
Action.Open = makeComponent("Action.Open");
Action.OpenWith = makeComponent("Action.OpenWith");
Action.Push = makeComponent("Action.Push");
Action.SubmitForm = makeComponent("Action.SubmitForm");
Action.Paste = makeComponent("Action.Paste");
Action.ShowInFinder = makeComponent("Action.ShowInFinder");
Action.Trash = makeComponent("Action.Trash");
Action.CreateSnippet = makeComponent("Action.CreateSnippet");
Action.CreateQuicklink = makeComponent("Action.CreateQuicklink");
Action.Style = { Regular: "regular", Destructive: "destructive" };

const ActionPanel = makeComponent("ActionPanel");
ActionPanel.Section = makeComponent("ActionPanel.Section");
ActionPanel.Submenu = makeComponent("ActionPanel.Submenu");

const Icon = new Proxy(
  {},
  { get: (_t, k) => (typeof k === "string" ? `icon:${k}` : k) }
);
const Color = new Proxy(
  {},
  { get: (_t, k) => (typeof k === "string" ? `color:${k}` : k) }
);
const Keyboard = { Shortcut: { Common: {} } };
const Image = {
  Mask: { Circle: "circle", RoundedRectangle: "rounded" },
};
const Alert = {
  ActionStyle: {
    Default: "default",
    Destructive: "destructive",
    Cancel: "cancel",
  },
};

async function confirmAlert(_opts) {
  return false;
}
async function showInFinder(p) {
  _captured.push(`[finder] ${p}`);
}

const AI = {
  async ask(prompt) {
    _captured.push(`[AI.ask] ${String(prompt).slice(0, 200)}`);
    return "";
  },
};

class Cache {
  constructor() {
    this._m = new Map();
  }
  get(k) {
    return this._m.get(k);
  }
  set(k, v) {
    this._m.set(k, v);
  }
  remove(k) {
    this._m.delete(k);
  }
  clear() {
    this._m.clear();
  }
  has(k) {
    return this._m.has(k);
  }
  subscribe() {
    return () => {};
  }
}

const OAuth = {
  PKCEClient: class {
    constructor(_opts) {}
    async authorizationRequest() {
      throw new Error("OAuth is not supported in OmniLauncher's Raycast shim.");
    }
    async authorize() {
      throw new Error("OAuth is not supported in OmniLauncher's Raycast shim.");
    }
    async setTokens() {}
    async getTokens() {
      return undefined;
    }
    async removeTokens() {}
    RedirectMethod: { Web: "web", AppURI: "app-uri" };
  },
};

module.exports = {
  __resetCapture,
  __getCapture,
  showToast,
  showHUD,
  closeMainWindow,
  popToRoot,
  Clipboard,
  open,
  Toast,
  getPreferenceValues,
  getSelectedText,
  getSelectedFinderItems,
  environment,
  LocalStorage,
  List,
  Detail,
  Form,
  Grid,
  Action,
  ActionPanel,
  Icon,
  Color,
  Keyboard,
  Image,
  Alert,
  confirmAlert,
  showInFinder,
  AI,
  Cache,
  OAuth,
};
