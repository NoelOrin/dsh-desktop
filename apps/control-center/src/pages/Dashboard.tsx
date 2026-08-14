import { createEffect, createSignal, onCleanup } from "solid-js";
import type { RuntimeSnapshot } from "@dsh-desktop/contracts";
import { getStatus, installDsh, onLog, onStatus, openLogDirectory, restart } from "../lib/ipc";

const SHOW_LOGS = new Set(["installing", "starting", "failed"]);
const MAX_LOGS = 500;

export default function Dashboard() {
  const [snapshot, setSnapshot] = createSignal<RuntimeSnapshot | null>(null);
  const [logs, setLogs] = createSignal<string[]>([]);
  let logsEl: HTMLPreElement | undefined;

  createEffect(() => {
    let disposed = false;
    getStatus().then((s) => {
      if (disposed) return;
      setSnapshot(s);
      setLogs((s.logs ?? []).slice(-MAX_LOGS));
    });
    const un1 = onStatus((s) => setSnapshot(s));
    const un2 = onLog((line) => setLogs((prev) => [...prev, line].slice(-MAX_LOGS)));
    onCleanup(() => {
      disposed = true;
      un1.then((u) => u());
      un2.then((u) => u());
    });
  });

  // 日志区自动滚动到底部，参照 apps/shell/src/main.ts 的 appendLog
  createEffect(() => {
    void logs();
    if (logsEl) logsEl.scrollTop = logsEl.scrollHeight;
  });

  const phase = () => snapshot()?.phase ?? "detecting";
  const message = () => snapshot()?.message ?? "正在检测运行环境...";
  const url = () => snapshot()?.url;
  const showLogs = () => SHOW_LOGS.has(phase());

  return (
    <section class="dashboard">
      <div class="status">
        <span class={`dot dot--${phase()}`} />
        <span class="status__message">{message()}</span>
        {url() && <code class="status__url">{url()}</code>}
      </div>
      <div class="actions">
        <button class="primary" hidden={phase() !== "missing"} onClick={() => installDsh().catch((e) => console.error(e))}>
          安装 DSH
        </button>
        <button hidden={phase() !== "failed"} onClick={() => restart().catch((e) => console.error(e))}>重试</button>
        <button hidden={phase() !== "failed"} onClick={() => openLogDirectory().catch((e) => console.error(e))}>日志目录</button>
      </div>
      <pre ref={logsEl} class="logs" hidden={!showLogs()}>{logs().join("\n")}</pre>
    </section>
  );
}
