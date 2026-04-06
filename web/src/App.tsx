import { useCallback, useEffect, useState } from "react"
import type { TursoConfig } from "@/lib/turso"
import { getCredentials, clearCredentials, validateCredentials } from "@/lib/auth"
import { ConnectionForm } from "@/components/ConnectionForm"
import { TaskTable } from "@/components/TaskTable"
import { Layout } from "@/components/Layout"

export default function App() {
  const [config, setConfig] = useState<TursoConfig | null>(null)
  const [checking, setChecking] = useState(true)

  useEffect(() => {
    const saved = getCredentials()
    if (saved) {
      validateCredentials(saved)
        .then(ok => { if (ok) setConfig(saved) })
        .catch(() => {})
        .finally(() => setChecking(false))
    } else {
      setChecking(false)
    }
  }, [])

  const handleDisconnect = useCallback(() => {
    clearCredentials()
    setConfig(null)
  }, [])

  if (checking) {
    return (
      <Layout connected={false} onDisconnect={handleDisconnect}>
        <div className="flex items-center justify-center p-8 text-muted-foreground">
          Checking connection...
        </div>
      </Layout>
    )
  }

  if (!config) {
    return (
      <Layout connected={false} onDisconnect={handleDisconnect}>
        <ConnectionForm onConnected={setConfig} />
      </Layout>
    )
  }

  return (
    <Layout connected={true} onDisconnect={handleDisconnect}>
      <TaskTable config={config} />
    </Layout>
  )
}
