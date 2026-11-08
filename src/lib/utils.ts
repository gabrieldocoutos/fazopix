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
