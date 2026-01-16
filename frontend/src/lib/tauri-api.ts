import { invoke } from '@tauri-apps/api/core'

/**
 * Server mode configuration - matches Rust `ServerMode` enum.
 * Uses tagged union serialization: `{ mode: "local" }` or `{ mode: "remote", url: "..." }`
 */
export type ServerMode = { mode: 'local' } | { mode: 'remote'; url: string }

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

export async function launchMcpServer(): Promise<void> {
  return invoke<void>('launch_mcp_server')
}

export async function stopMcpServer(): Promise<void> {
  return invoke<void>('stop_mcp_server')
}

export async function isMcpServerRunning(): Promise<boolean> {
  return invoke<boolean>('is_mcp_server_running')
}
