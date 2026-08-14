const TOOLS = [
  { key: "logs", title: "打开日志目录", desc: "打开 dsh 运行日志目录", action: "open_logs" },
];

export default function Tools() {
  return (
    <section class="tools">
      <h2>工具</h2>
      <ul>
        {TOOLS.map((tool) => (
          <li class="tool">
            <span class="tool__title">{tool.title}</span>
            <span class="tool__desc">{tool.desc}</span>
          </li>
        ))}
      </ul>
      <p class="hint">更多工具入口将在此扩展。</p>
    </section>
  );
}
