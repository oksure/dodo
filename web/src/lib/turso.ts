export interface TursoConfig {
  url: string
  token: string
}

interface TursoValue {
  type: string
  value: string | null
}

interface TursoCol {
  name: string
  decltype?: string
}

interface TursoRows {
  columns: TursoCol[]
  rows: TursoValue[][]
}

interface TursoResult {
  type: string
  response?: { result: TursoRows }
}

export interface QueryResult {
  columns: string[]
  rows: Record<string, string | null>[]
}

function parseResult(result: TursoResult): QueryResult {
  if (result.type !== "ok" || !result.response) {
    return { columns: [], rows: [] }
  }
  const { columns, rows } = result.response.result
  const colNames = columns.map(c => c.name)
  return {
    columns: colNames,
    rows: rows.map(row =>
      Object.fromEntries(row.map((v, i) => [colNames[i], v.value]))
    ),
  }
}

export async function query(
  config: TursoConfig,
  sql: string,
): Promise<QueryResult> {
  const res = await fetch(`${config.url}/v2/pipeline`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${config.token}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      requests: [
        { type: "execute", stmt: { sql } },
        { type: "close" },
      ],
    }),
  })
  if (!res.ok) {
    const body = await res.text()
    console.error(`Turso HTTP ${res.status}:`, body)
    throw new Error(`Connection failed (HTTP ${res.status})`)
  }
  const data = await res.json()
  return parseResult(data.results[0])
}

export async function testConnection(config: TursoConfig): Promise<boolean> {
  const result = await query(config, "SELECT 1")
  return result.rows.length > 0
}
