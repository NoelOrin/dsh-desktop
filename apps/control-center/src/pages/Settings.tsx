import { createEffect, createSignal, onCleanup } from "solid-js";
import type { DshConfig } from "@dsh-desktop/contracts";
import { getConfig, restart, setConfig } from "../lib/ipc";

export default function Settings() {
  const [dshBin, setDshBin] = createSignal("");
  const [dshNode, setDshNode] = createSignal("");
  const [dshHome, setDshHome] = createSignal("");
  const [saved, setSaved] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  createEffect(() => {
    let disposed = false;
    getConfig()
      .then((c) => {
        if (disposed) return;
        setDshBin(c.dsh_bin ?? "");
        setDshNode(c.dsh_node ?? "");
        setDshHome(c.dsh_home ?? "");
      })
      .catch((e) => {
        if (disposed) return;
        setError(`读取配置失败: ${String(e)}`);
      });
    onCleanup(() => {
      disposed = true;
    });
  });

  const save = async () => {
    const config: DshConfig = {
      dsh_bin: dshBin() || null,
      dsh_node: dshNode() || null,
      dsh_home: dshHome() || null,
    };
    try {
      await setConfig(config);
      setSaved(true);
      setError(null);
    } catch (e) {
      setSaved(false);
      setError(`保存失败: ${String(e)}`);
    }
  };

  return (
    <section class="settings">
      <label class="field">
        <span>DSH 入口 (DSH_BIN)</span>
        <input value={dshBin()} onInput={(e) => { setDshBin(e.currentTarget.value); setSaved(false); setError(null); }} placeholder="例如 /path/to/dsh" />
      </label>
      <label class="field">
        <span>Node 解释器 (DSH_NODE)</span>
        <input value={dshNode()} onInput={(e) => { setDshNode(e.currentTarget.value); setSaved(false); setError(null); }} placeholder="例如 /path/to/node" />
      </label>
      <label class="field">
        <span>Harness 数据目录 (DSH_HOME)</span>
        <input value={dshHome()} onInput={(e) => { setDshHome(e.currentTarget.value); setSaved(false); setError(null); }} placeholder="留空则继承环境变量" />
      </label>
      <div class="settings__actions">
        <button class="primary" onClick={save}>保存</button>
        <button onClick={() => restart().catch((e) => console.error(e))}>重启 dsh 生效</button>
      </div>
      {error() && <p class="hint hint--error">{error()}</p>}
      {saved() && <p class="hint">已保存，重启 dsh 后生效。</p>}
    </section>
  );
}
