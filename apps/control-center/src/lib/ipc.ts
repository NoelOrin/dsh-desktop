import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { COMMANDS, EVENTS, type DshConfig, type RuntimeSnapshot } from "@dsh-desktop/contracts";

export function getStatus(): Promise<RuntimeSnapshot> {
  return invoke<RuntimeSnapshot>(COMMANDS.getStatus);
}

export function installDsh(): Promise<void> {
  return invoke(COMMANDS.installDsh);
}

export function restart(): Promise<void> {
  return invoke(COMMANDS.restart);
}

export function openLogDirectory(): Promise<void> {
  return invoke(COMMANDS.openLogDirectory);
}

export function getConfig(): Promise<DshConfig> {
  return invoke<DshConfig>(COMMANDS.getConfig);
}

export function setConfig(config: DshConfig): Promise<void> {
  return invoke(COMMANDS.setConfig, { config });
}

export function onStatus(listener: (snapshot: RuntimeSnapshot) => void): Promise<UnlistenFn> {
  return listen<RuntimeSnapshot>(EVENTS.dshStatus, (event) => listener(event.payload));
}

export function onLog(listener: (line: string) => void): Promise<UnlistenFn> {
  return listen<string>(EVENTS.dshLog, (event) => listener(event.payload));
}
