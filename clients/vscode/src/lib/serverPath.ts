import * as path from "node:path";

export interface ResolveServerCommandDeps {
  envServerPath: string | undefined;
  extensionPath: string;
  platform: string;
  existsSync: (p: string) => boolean;
}

export function resolveServerCommand(deps: ResolveServerCommandDeps): string {
  if (deps.envServerPath) return deps.envServerPath;
  const binaryName = deps.platform === "win32" ? "badjuju.exe" : "badjuju";
  const bundled = path.join(deps.extensionPath, "out", "bin", binaryName);
  if (deps.existsSync(bundled)) return bundled;
  return "badjuju";
}
