export { cn } from "cn"

const brl = new Intl.NumberFormat("pt-BR", {
  style: "currency",
  currency: "BRL",
})

/** Formats integer cents as BRL, e.g. `123456` → `"R$ 1.234,56"`. */
export function formatBRL(cents: number) {
  return brl.format(cents / 100)
}

/** Formats an ISO date (`"YYYY-MM-DD"`) as `"dd/mm/yyyy"` without timezone shifts. */
export function formatDate(iso: string) {
  const [y, m, d] = iso.split("-")
  return `${d}/${m}/${y}`
}

/** The current month as `"YYYY-MM"`. */
export function currentMonth() {
  const now = new Date()
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}`
}

/** Formats `"YYYY-MM"` as e.g. `"novembro de 2026"`. */
export function monthLabel(month: string) {
  const [y, m] = month.split("-").map(Number)
  return new Intl.DateTimeFormat("pt-BR", {
    month: "long",
    year: "numeric",
  }).format(new Date(y, m - 1, 1))
}
