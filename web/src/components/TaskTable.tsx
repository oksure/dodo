import { useEffect, useMemo, useState } from "react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { query } from "@/lib/turso"
import type { TursoConfig, QueryResult } from "@/lib/turso"

interface Task {
  id: string
  num_id: number
  title: string
  status: string
  area: string
  project: string | null
  context: string | null
  priority: number
  estimate_minutes: number | null
  elapsed_seconds: number
  deadline: string | null
  scheduled: string | null
  created: string
  modified_at: string | null
  tags: string | null
}

type SortKey = keyof Pick<Task, "num_id" | "title" | "status" | "area" | "project" | "priority" | "elapsed_seconds" | "created">
type SortDir = "asc" | "desc"

function formatDuration(seconds: number): string {
  if (seconds <= 0) return "--"
  const h = Math.floor(seconds / 3600)
  const m = Math.floor((seconds % 3600) / 60)
  if (h > 0 && m > 0) return `${h}h ${m}m`
  if (h > 0) return `${h}h`
  if (m > 0) return `${m}m`
  return "<1m"
}

function formatEstimate(minutes: number | null): string {
  if (!minutes) return "--"
  if (minutes >= 60) {
    const h = Math.floor(minutes / 60)
    const m = minutes % 60
    return m > 0 ? `${h}h ${m}m` : `${h}h`
  }
  return `${minutes}m`
}

function statusBadge(status: string) {
  switch (status) {
    case "Running":
      return <Badge className="bg-green-600 text-white">Running</Badge>
    case "Paused":
      return <Badge className="bg-yellow-500 text-white">Paused</Badge>
    case "Done":
      return <Badge variant="secondary" className="line-through">Done</Badge>
    default:
      return <Badge variant="outline">Pending</Badge>
  }
}

function priorityLabel(p: number) {
  if (p <= 0) return null
  const labels = ["", "!", "!!", "!!!", "!!!!"]
  const colors = ["", "text-blue-500", "text-yellow-500", "text-orange-500", "text-red-500"]
  return <span className={`font-mono font-bold ${colors[p] || ""}`}>{labels[p] || ""}</span>
}

function formatDate(iso: string | null): string {
  if (!iso) return "--"
  const d = new Date(iso + "T00:00:00")
  const now = new Date()
  now.setHours(0, 0, 0, 0)
  const diff = Math.floor((d.getTime() - now.getTime()) / 86400000)
  if (diff === 0) return "Today"
  if (diff === 1) return "Tomorrow"
  if (diff === -1) return "Yesterday"
  if (diff > 1 && diff <= 6) return d.toLocaleDateString("en", { weekday: "short", month: "short", day: "numeric" })
  if (diff < -1 && diff >= -6) return `${-diff}d ago`
  return d.toLocaleDateString("en", { month: "short", day: "numeric" })
}

function areaGroup(task: Task): string {
  if (task.status === "Done") return "Done"
  if (!task.scheduled) return "Today"
  const sched = new Date(task.scheduled + "T00:00:00")
  const today = new Date()
  today.setHours(0, 0, 0, 0)
  const diffDays = Math.floor((sched.getTime() - today.getTime()) / 86400000)
  if (diffDays <= 0) return "Today"
  if (diffDays <= 7) return "This Week"
  return "Long Term"
}

const GROUP_ORDER: Record<string, number> = {
  "Today": 0,
  "This Week": 1,
  "Long Term": 2,
  "Done": 3,
}

function parseTask(row: Record<string, string | null>): Task {
  return {
    id: row.id || "",
    num_id: parseInt(row.num_id || "0", 10),
    title: row.title || "",
    status: row.status || "Pending",
    area: row.area || "",
    project: row.project || null,
    context: row.context || null,
    priority: parseInt(row.priority || "0", 10),
    estimate_minutes: row.estimate_minutes ? parseInt(row.estimate_minutes, 10) : null,
    elapsed_seconds: parseInt(row.elapsed_seconds || "0", 10),
    deadline: row.deadline || null,
    scheduled: row.scheduled || null,
    created: row.created || "",
    modified_at: row.modified_at || null,
    tags: row.tags || null,
  }
}

interface Props {
  config: TursoConfig
}

export function TaskTable({ config }: Props) {
  const [tasks, setTasks] = useState<Task[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [search, setSearch] = useState("")
  const [sortKey, setSortKey] = useState<SortKey>("num_id")
  const [sortDir, setSortDir] = useState<SortDir>("asc")
  const [refreshKey, setRefreshKey] = useState(0)

  useEffect(() => {
    let cancelled = false
    async function load() {
      setLoading(true)
      setError(null)
      try {
        const sql = `
          SELECT
            t.id, t.num_id, t.title, t.status, t.area, t.project, t.context,
            t.priority, t.estimate_minutes, t.deadline, t.scheduled, t.created,
            t.modified_at, t.tags,
            CASE
              WHEN t.status = 'Done' THEN COALESCE(t.elapsed_snapshot, 0)
              ELSE COALESCE(
                (SELECT SUM(
                  CASE WHEN s.end_time IS NULL
                    THEN CAST((julianday('now') - julianday(s.start_time)) * 86400 AS INTEGER)
                    ELSE CAST((julianday(s.end_time) - julianday(s.start_time)) * 86400 AS INTEGER)
                  END
                ) FROM sessions s WHERE s.task_id = t.id),
                0
              )
            END as elapsed_seconds
          FROM tasks t
          WHERE t.is_template = 0 OR t.is_template IS NULL
          ORDER BY t.num_id ASC
        `
        const result: QueryResult = await query(config, sql)
        if (!cancelled) {
          setTasks(result.rows.map(parseTask))
        }
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : "Failed to load tasks")
        }
      } finally {
        if (!cancelled) setLoading(false)
      }
    }
    load()
    return () => { cancelled = true }
  }, [config, refreshKey])

  const handleSort = (key: SortKey) => {
    if (sortKey === key) {
      setSortDir(d => d === "asc" ? "desc" : "asc")
    } else {
      setSortKey(key)
      setSortDir("asc")
    }
  }

  const sortIndicator = (key: SortKey) => {
    if (sortKey !== key) return null
    return sortDir === "asc" ? " \u2191" : " \u2193"
  }

  const filtered = useMemo(() => {
    if (!search.trim()) return tasks
    const terms = search.toLowerCase().split(/\s+/)
    return tasks.filter(t => {
      const haystack = [
        t.title, t.project, t.context, t.tags, t.status,
      ].filter(Boolean).join(" ").toLowerCase()
      return terms.every(term => {
        if (term.startsWith("+") && term.length > 1)
          return (t.project || "").toLowerCase().includes(term.slice(1))
        if (term.startsWith("@") && term.length > 1)
          return (t.context || "").toLowerCase().includes(term.slice(1))
        return haystack.includes(term)
      })
    })
  }, [tasks, search])

  const grouped = useMemo(() => {
    const sorted = [...filtered].sort((a, b) => {
      let cmp = 0
      switch (sortKey) {
        case "num_id": cmp = a.num_id - b.num_id; break
        case "title": cmp = a.title.localeCompare(b.title); break
        case "status": cmp = a.status.localeCompare(b.status); break
        case "area": cmp = areaGroup(a).localeCompare(areaGroup(b)); break
        case "project": cmp = (a.project || "").localeCompare(b.project || ""); break
        case "priority": cmp = a.priority - b.priority; break
        case "elapsed_seconds": cmp = a.elapsed_seconds - b.elapsed_seconds; break
        case "created": cmp = a.created.localeCompare(b.created); break
      }
      return sortDir === "desc" ? -cmp : cmp
    })

    const groups: { label: string; tasks: Task[] }[] = []
    const byGroup: Record<string, Task[]> = {}
    for (const t of sorted) {
      const g = areaGroup(t)
      if (!byGroup[g]) byGroup[g] = []
      byGroup[g].push(t)
    }
    for (const label of ["Today", "This Week", "Long Term", "Done"]) {
      if (byGroup[label]?.length) {
        groups.push({ label, tasks: byGroup[label] })
      }
    }
    return groups
  }, [filtered, sortKey, sortDir])

  if (loading) {
    return <div className="flex items-center justify-center p-8 text-muted-foreground">Loading tasks...</div>
  }

  if (error) {
    return <div className="p-8 text-center text-destructive">{error}</div>
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2">
        <Input
          placeholder="Search tasks... (+project, @context, or text)"
          value={search}
          onChange={e => setSearch(e.target.value)}
          className="max-w-sm"
        />
        <Button
          variant="outline"
          size="sm"
          onClick={() => setRefreshKey(k => k + 1)}
          disabled={loading}
        >
          {loading ? "Loading..." : "Refresh"}
        </Button>
      </div>
      <div className="text-sm text-muted-foreground">
        {filtered.length} task{filtered.length !== 1 ? "s" : ""}
      </div>
      {grouped.map(group => (
        <div key={group.label}>
          <h3 className="mb-2 text-sm font-semibold tracking-wide text-muted-foreground uppercase">
            {group.label}
            <span className="ml-2 text-xs font-normal">({group.tasks.length})</span>
          </h3>
          <div className="rounded-md border overflow-x-auto">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="w-16 cursor-pointer select-none" onClick={() => handleSort("num_id")}>
                    #{sortIndicator("num_id")}
                  </TableHead>
                  <TableHead className="cursor-pointer select-none" onClick={() => handleSort("title")}>
                    Title{sortIndicator("title")}
                  </TableHead>
                  <TableHead className="w-24 cursor-pointer select-none" onClick={() => handleSort("status")}>
                    Status{sortIndicator("status")}
                  </TableHead>
                  <TableHead className="w-16 cursor-pointer select-none" onClick={() => handleSort("priority")}>
                    Pri{sortIndicator("priority")}
                  </TableHead>
                  <TableHead className="w-28 cursor-pointer select-none" onClick={() => handleSort("project")}>
                    Project{sortIndicator("project")}
                  </TableHead>
                  <TableHead className="w-24">Estimate</TableHead>
                  <TableHead className="w-24 cursor-pointer select-none" onClick={() => handleSort("elapsed_seconds")}>
                    Elapsed{sortIndicator("elapsed_seconds")}
                  </TableHead>
                  <TableHead className="w-24">Deadline</TableHead>
                  <TableHead className="w-24">Scheduled</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {group.tasks.map(t => (
                  <TableRow key={t.id} className={t.status === "Done" ? "opacity-60" : ""}>
                    <TableCell className="font-mono text-xs text-muted-foreground">{t.num_id}</TableCell>
                    <TableCell>
                      <span className={t.status === "Done" ? "line-through" : ""}>
                        {t.title}
                      </span>
                      {t.context && (
                        <span className="ml-2 text-xs text-muted-foreground">@{t.context}</span>
                      )}
                      {t.tags && (
                        <span className="ml-2 text-xs text-muted-foreground">#{t.tags}</span>
                      )}
                    </TableCell>
                    <TableCell>{statusBadge(t.status)}</TableCell>
                    <TableCell>{priorityLabel(t.priority)}</TableCell>
                    <TableCell>
                      {t.project && (
                        <span className="text-sm text-purple-600 dark:text-purple-400">+{t.project}</span>
                      )}
                    </TableCell>
                    <TableCell className="text-xs text-muted-foreground">
                      {formatEstimate(t.estimate_minutes)}
                    </TableCell>
                    <TableCell className="text-xs font-mono">
                      {formatDuration(t.elapsed_seconds)}
                    </TableCell>
                    <TableCell className="text-xs text-muted-foreground">
                      {formatDate(t.deadline)}
                    </TableCell>
                    <TableCell className="text-xs text-muted-foreground">
                      {formatDate(t.scheduled)}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        </div>
      ))}
      {grouped.length === 0 && (
        <div className="py-12 text-center text-muted-foreground">
          {search ? "No tasks match your search" : "No tasks found"}
        </div>
      )}
    </div>
  )
}
