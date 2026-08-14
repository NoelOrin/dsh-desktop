import { createEffect, createSignal, onCleanup } from "solid-js";
import type { RuntimeSnapshot } from "@dsh-desktop/contracts";
import { getStatus, installDsh, onLog, onStatus, openLogDirectory, restart } from "../lib/ipc";

const SHOW_LOGS = new Set(["installing", "starting", "failed"]);

export default function Dashboard() {
  const [snapshot, setSnapshot] = createSignal<RuntimeSnapshot | null>(null);
  const [logs, setLogs] = createSignal<string[]>([]);

  createEffect(() => {
    let disposed = false;
    getStatus().then((s) => {
      if (disposed) return;
      setSnapshot(s);
      setLogs(s.logs ?? []);
    });
    const un1 = onStatus((s) => setSnapshot(s));
    const un2 = onLog((line) => setLogs((prev) => [...prev, line]));
    onCleanup(() => {
      disposed = true;
      un1.then((u) => u());
      un2.then((u) => u());
    });
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
      <pre class="logs" hidden={!showLogs()}>{logs().join("\n")}</pre>
    </section>
  );
}
