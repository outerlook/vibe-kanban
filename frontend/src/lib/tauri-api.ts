import { invoke } from '@tauri-apps/api/core'

export type ServerMode = 'Embedded' | 'Remote'

declare global {
  interface Window {
    __TAURI__?: unknown
    __TAURI_INTERNALS__?: unknown
  }
}

export function isTauriEnvironment(): boolean {
  return (
    typeof window !== 'undefined' &&
    (window.__TAURI__ !== undefined || window.__TAURI_INTERNALS__ !== undefined)
  )
}

export async function getServerUrl(): Promise<string> {
  return invoke<string>('get_server_url')
}

export async function getServerMode(): Promise<ServerMode> {
  return invoke<ServerMode>('get_server_mode')
}

export async function setServerMode(mode: ServerMode): Promise<void> {
  return invoke<void>('set_server_mode', { mode })
}

export async function getRemoteUrl(): Promise<string | null> {
  return invoke<string | null>('get_remote_url')
}

export async function setRemoteUrl(url: string): Promise<void> {
  return invoke<void>('set_remote_url', { url })
}
