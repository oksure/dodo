import type { ReactNode } from "react"
import { Button } from "@/components/ui/button"

interface Props {
  connected: boolean
  onDisconnect: () => void
  children: ReactNode
}

export function Layout({ connected, onDisconnect, children }: Props) {
  return (
    <div className="min-h-screen">
      <header className="sticky top-0 z-10 border-b bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
        <div className="mx-auto flex h-14 max-w-5xl items-center justify-between px-4">
          <div className="flex items-center gap-3">
            <h1 className="text-lg font-semibold tracking-tight">dodo</h1>
            {connected && (
              <span className="flex items-center gap-1.5 text-xs text-green-600">
                <span className="inline-block h-2 w-2 rounded-full bg-green-500" />
                Connected
              </span>
            )}
          </div>
          {connected && (
            <Button variant="ghost" size="sm" onClick={onDisconnect}>
              Disconnect
            </Button>
          )}
        </div>
      </header>
      <main className="mx-auto max-w-5xl px-4 py-6">
        {children}
      </main>
    </div>
  )
}
