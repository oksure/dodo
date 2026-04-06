import type { TursoConfig } from "./turso"
import { testConnection } from "./turso"

const STORAGE_KEY = "dodo-turso-credentials"
const MAX_AGE_MS = 7 * 24 * 60 * 60 * 1000 // 7 days

export function getCredentials(): TursoConfig | null {
  const raw = localStorage.getItem(STORAGE_KEY)
  if (!raw) return null
  try {
    const parsed = JSON.parse(raw)
    if (!parsed.url || !parsed.token) return null
    if (parsed.stored_at && Date.now() - parsed.stored_at > MAX_AGE_MS) {
      localStorage.removeItem(STORAGE_KEY)
      return null
    }
    return { url: parsed.url, token: parsed.token }
  } catch {
    return null
  }
}

export function setCredentials(config: TursoConfig): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify({ ...config, stored_at: Date.now() }))
}

export function clearCredentials(): void {
  localStorage.removeItem(STORAGE_KEY)
}

export async function validateCredentials(config: TursoConfig): Promise<boolean> {
  return testConnection(config)
}
