import { injectPluginCss } from "../../client-kit/inject";
import { apply as applyModels, inject as modelsInject } from "./models/client/index";

injectPluginCss("@dsh-desktop/plugin-reasoning", "@dsh-desktop/plugin-reasoning/ui");

type ModelsApply = typeof applyModels;

const appliedContexts = new WeakSet<object>();

/** 复用参考实现后的完整模型设置页：保留官方 Models 页能力并叠加思考强度勾选。 */
export const inject = modelsInject;

export function apply(ctx: Parameters<ModelsApply>[0]): void {
  const key = ctx as unknown as object;
  if (appliedContexts.has(key)) return;
  appliedContexts.add(key);
  applyModels(ctx);
}
