import { type AdvancedLayoutState, DEFAULT_LAYOUT_STATE } from "./layout-state";

/** advanced 布局的轻量可订阅状态。 */
export class AdvancedLayoutService {
  private state: AdvancedLayoutState = DEFAULT_LAYOUT_STATE;
  private listeners = new Set<() => void>();

  getState(): AdvancedLayoutState {
    return { ...this.state };
  }

  setState(patch: Partial<AdvancedLayoutState>): void {
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener();
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
}
