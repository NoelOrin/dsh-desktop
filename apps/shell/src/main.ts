import type { RuntimePhase, RuntimeSnapshot, UiThemeSnapshot } from "@dsh-desktop/contracts";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

const messageEl = document.getElementById("message") as HTMLSpanElement;
const statusEl = document.getElementById("status") as HTMLDivElement;
const installBtn = document.getElementById("install") as HTMLButtonElement;
const retryBtn = document.getElementById("retry") as HTMLButtonElement;
const openLogsBtn = document.getElementById("open-logs") as HTMLButtonElement;
const logsEl = document.getElementById("logs") as HTMLPreElement;

const SHOW_LOGS = new Set<RuntimePhase>(["installing", "starting", "failed"]);

function isTauri(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

function render(status: Partial<RuntimeSnapshot>): void {
  const phase = (status.phase ?? "detecting") as RuntimePhase;
  statusEl.dataset.phase = phase;
  messageEl.textContent = status.message || "DSH 状态未知";

  installBtn.classList.toggle("hidden", phase !== "missing" || status.node_found === false);
  retryBtn.classList.toggle("hidden", phase !== "failed");
  openLogsBtn.classList.toggle("hidden", phase !== "failed");
  logsEl.classList.toggle("hidden", !SHOW_LOGS.has(phase));

  if (phase === "ready" && status.url) {
    window.location.href = status.url;
  }
}

function appendLog(line: string): void {
  logsEl.textContent += `${line}\n`;
  logsEl.scrollTop = logsEl.scrollHeight;
}

function applyUiTheme(snapshot: UiThemeSnapshot): void {
  const root = document.documentElement;
  const tokens = snapshot.tokens;
  root.style.setProperty("--theme-bg", tokens.bg);
  root.style.setProperty("--theme-ink", tokens.fg);
  root.style.setProperty("--theme-muted", tokens.muted);
  root.style.setProperty("--theme-line", tokens.line);
  root.style.setProperty("--theme-accent", tokens.accent);
  root.style.setProperty("--theme-accent-ink", tokens.accent);
  root.style.setProperty("--theme-button-fg", tokens.button_fg);
  root.style.setProperty("--theme-field", tokens.field);
  root.style.colorScheme = tokens.scheme;
}

async function initTheme(): Promise<void> {
  if (!isTauri()) return;
  try {
    const snapshot = await invoke<UiThemeSnapshot>("get_ui_theme");
    applyUiTheme(snapshot);
  } catch {
    // 忽略：主题不可用时保持样式表默认
  }
  await listen<UiThemeSnapshot>("dsh-ui-theme", (event) => applyUiTheme(event.payload));
}

async function init(): Promise<void> {
  if (!isTauri()) {
    render({ phase: "detecting", message: "正在检测运行环境..." });
    return;
  }

  try {
    const status = await invoke<RuntimeSnapshot>("get_status");
    if (status.logs?.length) {
      logsEl.textContent = status.logs.join("\n");
      logsEl.scrollTop = logsEl.scrollHeight;
    }
    render(status);
  } catch (error) {
    render({ phase: "failed", message: String(error) });
  }

  await initTheme();
  await listen<RuntimeSnapshot>("dsh-status", (event) => render(event.payload));
  await listen<string>("dsh-log", (event) => appendLog(String(event.payload)));
}

installBtn.addEventListener("click", () => {
  render({ phase: "installing", message: "正在安装 DeepSeek Harness..." });
  invoke("install_dsh").catch((error) => {
    render({ phase: "failed", message: String(error) });
  });
});

retryBtn.addEventListener("click", () => {
  render({ phase: "detecting", message: "正在重新检测运行环境..." });
  invoke("restart").catch((error) => {
    render({ phase: "failed", message: String(error) });
  });
});

openLogsBtn.addEventListener("click", () => {
  invoke("open_log_directory").catch((error) => {
    render({ phase: "failed", message: String(error) });
  });
});

init().catch((error) => {
  render({ phase: "failed", message: String(error) });
});
