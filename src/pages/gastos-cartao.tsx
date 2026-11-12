import { useEffect, useState } from "react";
import { Link } from "react-router";
import { invoke } from "@tauri-apps/api/core";
import { CreditCardReaderIcon } from "lucide-react";
import { currentMonth, formatBRL, formatDate, monthLabel } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

/** Mirrors `MonthTransaction` in src-tauri/src/lib.rs. */
type MonthTransaction = {
  id: number;
  date: string; // YYYY-MM-DD
  description: string;
  amount_cents: number;
  card: string | null;
  installment: { current: number; total: number } | null;
  issuer: string;
};

const COLUMNS = ["#", "Data","Descrição", "Emissor", "Cartão", "Parcela", "Valor"];

/** A real split (N > 1) billed this month; estornos are left out. */
function isParcelado(t: MonthTransaction) {
  return !!t.installment && t.installment.total > 1 && t.amount_cents > 0;
}

export default function GastosCartao() {
  const [month, setMonth] = useState(currentMonth);
  const [transactions, setTransactions] = useState<MonthTransaction[] | null>(
    null,
  );
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    invoke<MonthTransaction[]>("list_transactions", { month }).then((data) => {
      if (cancelled) return;
      setTransactions(data);
      setLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, [month]);

  return (
    <div className="flex flex-col gap-4">
      <Card>
        <CardHeader>
          <CardTitle>Gastos cartão de crédito</CardTitle>
          <CardDescription>
            Transações das faturas com vencimento no mês selecionado.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="flex max-w-xs flex-col gap-2">
            <Label htmlFor="month">Mês da fatura</Label>
            <Input
              id="month"
              type="month"
              value={month}
              onChange={(e) => {
                const value = e.currentTarget.value;
                if (value) setMonth(value);
              }}
            />
          </div>
        </CardContent>
      </Card>

      {loading || !transactions ? (
        <LoadingTable />
      ) : transactions.length === 0 ? (
        <EmptyState month={month} />
      ) : (
        <TransactionsTable transactions={transactions} />
      )}
    </div>
  );
}

function LoadingTable() {
  return (
    <Card>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              {COLUMNS.map((c) => (
                <TableHead key={c}>{c}</TableHead>
              ))}
            </TableRow>
          </TableHeader>
          <TableBody>
            {Array.from({ length: 5 }, (_, i) => (
              <TableRow key={i}>
                {COLUMNS.map((c) => (
                  <TableCell key={c}>
                    <Skeleton className="h-4 w-full" />
                  </TableCell>
                ))}
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}

function EmptyState({ month }: { month: string }) {
  return (
    <Card>
      <CardContent className="flex flex-col items-center gap-3 py-10 text-center">
        <CreditCardReaderIcon className="size-10 text-muted-foreground" />
        <div className="flex flex-col gap-1">
          <p className="font-medium">
            Nenhuma transação em {monthLabel(month)}
          </p>
          <p className="text-sm text-muted-foreground">
            Envie a fatura com vencimento neste mês para ver os gastos.
          </p>
        </div>
        <Button asChild variant="outline">
          <Link to="/upload-fatura">Enviar fatura</Link>
        </Button>
      </CardContent>
    </Card>
  );
}

function TransactionsTable({
  transactions,
}: {
  transactions: MonthTransaction[];
}) {
  const [onlyParcelado, setOnlyParcelado] = useState(false);

  const total = transactions.reduce((sum, t) => sum + t.amount_cents, 0);
  const parcelado = transactions
    .filter(isParcelado)
    .reduce((sum, t) => sum + t.amount_cents, 0);
  const visible = onlyParcelado
    ? transactions.filter(isParcelado)
    : transactions;

  return (
    <Card>
      <CardHeader>
        <div className="flex flex-col gap-4 sm:flex-row sm:gap-12">
          <div className="flex flex-col gap-1.5">
            <CardDescription>Total de gastos</CardDescription>
            <CardTitle className="text-2xl tabular-nums">
              {formatBRL(total)}
            </CardTitle>
          </div>
          <div className="flex flex-col gap-1.5">
            <CardDescription>Total parcelado</CardDescription>
            <CardTitle className="text-2xl tabular-nums">
              {formatBRL(parcelado)}
            </CardTitle>
          </div>
        </div>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <Label className="w-fit cursor-pointer font-normal">
          <Checkbox
            checked={onlyParcelado}
            onCheckedChange={(checked) => setOnlyParcelado(checked === true)}
          />
          Mostrar apenas compras parceladas
        </Label>
        <Table>
          <TableHeader>
            <TableRow>
              {COLUMNS.map((c) => (
                <TableHead
                  key={c}
                  className={c === "Valor" ? "text-right" : undefined}
                >
                  {c}
                </TableHead>
              ))}
            </TableRow>
          </TableHeader>
          <TableBody>
            {visible.length === 0 && (
              <TableRow>
                <TableCell
                  colSpan={COLUMNS.length}
                  className="py-6 text-center text-muted-foreground"
                >
                  Nenhuma compra parcelada neste mês.
                </TableCell>
              </TableRow>
            )}
            {visible.map((t, i) => (
              <TableRow key={t.id}>
                <TableCell className="text-muted-foreground tabular-nums">
                  {i + 1}
                </TableCell>
                <TableCell className="tabular-nums">
                  {formatDate(t.date)}
                </TableCell>
                <TableCell>{t.description}</TableCell>
                <TableCell>{t.issuer}</TableCell>
                <TableCell className="tabular-nums">
                  {t.card ? `•••• ${t.card}` : "—"}
                </TableCell>
                <TableCell className="tabular-nums">
                  {t.installment
                    ? `${t.installment.current}/${t.installment.total}`
                    : "—"}
                </TableCell>
                <TableCell className="text-right tabular-nums">
                  {formatBRL(t.amount_cents)}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}
