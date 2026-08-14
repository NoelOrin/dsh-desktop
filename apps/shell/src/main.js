import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

const messageEl = document.getElementById("message");
const statusEl = document.getElementById("status");
const installBtn = document.getElementById("install");
const retryBtn = document.getElementById("retry");
const openLogsBtn = document.getElementById("open-logs");
const logsEl = document.getElementById("logs");

const SHOW_LOGS = new Set(["installing", "starting", "failed"]);

function isTauri() {
  return "__TAURI_INTERNALS__" in window;
}

function render(status) {
  const phase = status.phase ?? "detecting";
  statusEl.dataset.phase = phase;
  messageEl.textContent = status.message || "DSH 状态未知";

  installBtn.classList.toggle("hidden", phase !== "missing");
  retryBtn.classList.toggle("hidden", phase !== "failed");
  openLogsBtn.classList.toggle("hidden", phase !== "failed");
  logsEl.classList.toggle("hidden", !SHOW_LOGS.has(phase));

  if (phase === "ready" && status.url) {
    window.location.href = status.url;
  }
}

function appendLog(line) {
  logsEl.textContent += `${line}\n`;
  logsEl.scrollTop = logsEl.scrollHeight;
}

async function init() {
  if (!isTauri()) {
    render({ phase: "detecting", message: "正在检测运行环境..." });
    return;
  }

  try {
    const status = await invoke("get_status");
    if (status.logs?.length) {
      logsEl.textContent = status.logs.join("\n");
      logsEl.scrollTop = logsEl.scrollHeight;
    }
    render(status);
  } catch (error) {
    render({ phase: "failed", message: String(error) });
  }

  await listen("dsh-status", (event) => render(event.payload));
  await listen("dsh-log", (event) => appendLog(String(event.payload)));
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
