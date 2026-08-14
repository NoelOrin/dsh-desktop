// dsh-desktop: 让 Harness Web UI 顶部区域透明，透出 macOS 毛玻璃（vibrancy）。
// 由 Rust 侧在每次页面加载完成时通过 eval 注入；只注入一次。
// 注意：选择器基于当前 GUI 构建的 CSS Module 哈希类名，GUI 升级后可能失效（无害，只是不再透明）。
(function () {
  var TAG = "dsh-desktop-frosted-titlebar";
  if (document.getElementById(TAG)) return;

  var css = [
    /* 页面与三栏框架背景改为透明，露出窗口毛玻璃 */
    "html, body { background: transparent !important; }",
    ".pI_x6G_frame { background: transparent !important; }",
    ".pI_x6G_detailsCol { background: var(--dsw-alias-bg-base, #ffffff) !important; }",
    /* 会话列根背景透明：顶部操作 bar 区域变成毛玻璃 */
    ".wSkVaW_root { background: transparent !important; }",
    ".wSkVaW_header { background: transparent !important; }",
    /* 聊天消息区与输入区保持不透明，保证可读性 */
    ".wSkVaW_scrollBody { background: var(--dsw-alias-bg-base, #ffffff) !important; }",
    /* 侧栏保持自身不透明底色 */
    ".pI_x6G_sidebarCol { background: var(--dsw-specific-sidebar-fill, var(--dsw-alias-bg-base, #f7f8fa)) !important; }"
  ].join("\n");

  var style = document.createElement("style");
  style.id = TAG;
  style.textContent = css;
  (document.head || document.documentElement).appendChild(style);
})();
