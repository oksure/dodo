import { useState } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { setCredentials, validateCredentials } from "@/lib/auth"
import type { TursoConfig } from "@/lib/turso"

interface Props {
  onConnected: (config: TursoConfig) => void
}

export function ConnectionForm({ onConnected }: Props) {
  const [url, setUrl] = useState("")
  const [token, setToken] = useState("")
  const [testing, setTesting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setError(null)
    setTesting(true)

    const trimmedUrl = url.trim().replace(/\/$/, "")
    const config: TursoConfig = { url: trimmedUrl, token: token.trim() }

    try {
      const ok = await validateCredentials(config)
      if (ok) {
        setCredentials(config)
        onConnected(config)
      } else {
        setError("Connection succeeded but returned no data")
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Connection failed")
    } finally {
      setTesting(false)
    }
  }

  return (
    <div className="flex min-h-screen items-center justify-center p-4">
      <Card className="w-full max-w-md">
        <CardHeader>
          <CardTitle>Connect to Dodo</CardTitle>
          <CardDescription>
            Enter your Turso database URL and auth token to browse tasks.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-2">
              <label htmlFor="url" className="text-sm font-medium">
                Database URL
              </label>
              <Input
                id="url"
                type="url"
                placeholder="https://your-db.turso.io"
                value={url}
                onChange={e => setUrl(e.target.value)}
                required
              />
            </div>
            <div className="space-y-2">
              <label htmlFor="token" className="text-sm font-medium">
                Auth Token
              </label>
              <Input
                id="token"
                type="password"
                placeholder="eyJ..."
                value={token}
                onChange={e => setToken(e.target.value)}
                required
              />
            </div>
            {error && (
              <p className="text-sm text-destructive">{error}</p>
            )}
            <Button type="submit" className="w-full" disabled={testing}>
              {testing ? "Testing connection..." : "Connect"}
            </Button>
          </form>
        </CardContent>
      </Card>
    </div>
  )
}
