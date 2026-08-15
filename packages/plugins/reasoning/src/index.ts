export const name = "reasoning";

/** pi-ai 各思考等级的显示名；xhigh / max 用参考实现的用户友好文案。 */
const THINKING_LEVEL_LABELS: Record<string, string> = {
  off: "Off",
  minimal: "Minimal",
  low: "Low",
  medium: "Medium",
  high: "High",
  xhigh: "Very High",
  max: "Extreme",
};

interface ReasoningEffortLike {
  id: string;
  name: string;
  description?: string;
}

interface LlmResolvedModelLike {
  reasoning?: {
    efforts: readonly ReasoningEffortLike[];
    defaultEffort?: string;
  };
}

interface LlmLike {
  resolveModelInfo(
    provider: string,
    model: string,
    signal?: AbortSignal,
  ): Promise<LlmResolvedModelLike>;
}

interface ReasoningInjectContext {
  llm: LlmLike;
  effect(execute: () => unknown, label?: string): unknown;
}

interface ReasoningHostContext {
  inject(
    dependencies: readonly string[],
    callback: (ctx: ReasoningInjectContext) => unknown,
  ): unknown;
}

export function apply(ctx: unknown): void {
  // 模型设置 UI 由 client 面提供；host 面负责修正输入栏的推理等级显示名。
  (ctx as ReasoningHostContext).inject(["llm"] as never, (sctx) => {
    sctx.effect(() => {
      const original = sctx.llm.resolveModelInfo.bind(sctx.llm);
      const wrapped: LlmLike["resolveModelInfo"] = async (provider, model, signal) => {
        const info = await original(provider, model, signal);
        if (info.reasoning === undefined) return info;
        return {
          ...info,
          reasoning: {
            ...info.reasoning,
            efforts: info.reasoning.efforts.map((effort) => ({
              ...effort,
              name: THINKING_LEVEL_LABELS[effort.id] ?? effort.name,
            })),
          },
        };
      };
      sctx.llm.resolveModelInfo = wrapped;
      return () => {
        if (sctx.llm.resolveModelInfo === wrapped) {
          sctx.llm.resolveModelInfo = original;
        }
      };
    }, "reasoning: 修正模型推理等级显示名");
  });
}
