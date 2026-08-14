import { createSignal } from "solid-js";
import Dashboard from "./pages/Dashboard";
import Settings from "./pages/Settings";
import Tools from "./pages/Tools";

type Tab = "dashboard" | "settings" | "tools";

export default function App() {
  const [tab, setTab] = createSignal<Tab>("dashboard");
  return (
    <div class="app">
      <header class="app__header">
        <h1>DSH 控制中心</h1>
        <nav>
          <button
            class={tab() === "dashboard" ? "tab active" : "tab"}
            onClick={() => setTab("dashboard")}
          >
            仪表盘
          </button>
          <button
            class={tab() === "settings" ? "tab active" : "tab"}
            onClick={() => setTab("settings")}
          >
            设置
          </button>
          <button
            class={tab() === "tools" ? "tab active" : "tab"}
            onClick={() => setTab("tools")}
          >
            工具
          </button>
        </nav>
      </header>
      <main class="app__main">
        {tab() === "dashboard" && <Dashboard />}
        {tab() === "settings" && <Settings />}
        {tab() === "tools" && <Tools />}
      </main>
    </div>
  );
}
