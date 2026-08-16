/** advanced 三栏布局的纯状态计算。 */
export interface AdvancedLayoutState {
  sidebarOpen: boolean;
  detailsOpen: boolean;
  sidebarWidth: number;
  detailsWidth: number;
}

export interface DesktopViewport {
  width: number;
}

export interface DesktopColumns {
  sidebar: number;
  conversation: number;
  details: number;
}

const MIN_SIDEBAR = 220;
const MIN_DETAILS = 280;
const MIN_CONVERSATION = 360;
const NARROW_BREAKPOINT = 960;

export const DEFAULT_LAYOUT_STATE: AdvancedLayoutState = {
  sidebarOpen: true,
  detailsOpen: true,
  sidebarWidth: 280,
  detailsWidth: 360,
};

/** 根据视口和开关计算三栏宽度；窄屏自动收起 details。 */
export function computeDesktopColumns(
  state: AdvancedLayoutState,
  viewport: DesktopViewport,
): DesktopColumns {
  const sidebar = state.sidebarOpen ? Math.max(MIN_SIDEBAR, state.sidebarWidth) : 0;
  const details =
    state.detailsOpen && viewport.width >= NARROW_BREAKPOINT
      ? Math.max(MIN_DETAILS, state.detailsWidth)
      : 0;
  const conversation = Math.max(
    MIN_CONVERSATION,
    viewport.width - sidebar - details - (sidebar > 0 ? 8 : 0) - (details > 0 ? 8 : 0),
  );
  return { sidebar, conversation, details };
}
