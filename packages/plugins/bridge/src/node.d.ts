declare module "node:fs" {
  export function readFileSync(path: URL, options?: { encoding: "utf8" }): string;
}
