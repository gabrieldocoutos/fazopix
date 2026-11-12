import { useCallback, useEffect, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { BanknoteArrowUpIcon, Trash2Icon } from "lucide-react";
import { toast } from "sonner";
import { currentMonth, formatBRL, monthLabel } from "@/lib/utils";
import { Button } from "@/components/ui/button";
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

/** Mirrors `Income` in src-tauri/src/db.rs. */
type Income = {
  id: number;
  year: number;
  month: number;
  description: string;
  amount_cents: number;
};

type NewIncome = { month: string; description: string; amountCents: number };

const COLUMNS = ["Mês", "Descrição", "Valor", ""];

/**
 * Parses a typed amount ("5000,50", "5.000,50", "5000.50", "R$ 5.000") into
 * cents. The last `,` or `.` followed by 1–2 digits is the decimal separator;
 * any other separators are thousand separators. Returns `null` if the amount is
 * invalid or not positive.
 */
function parseAmountToCents(input: string): number | null {
  const s = input.replace(/R\$/i, "").replace(/\s/g, "");
  if (!/^[\d.,]+$/.test(s)) return null;
  const match = s.match(/^(.*?)(?:[.,](\d{1,2}))?$/);
  if (!match) return null;
  const intPart = match[1].replace(/[.,]/g, "");
  const decPart = (match[2] ?? "").padEnd(2, "0");
  if (!/^\d+$/.test(intPart)) return null;
  const cents = Number(intPart) * 100 + Number(decPart);
  return Number.isSafeInteger(cents) && cents > 0 ? cents : null;
}

function incomeMonth(i: Income) {
  return `${i.year}-${String(i.month).padStart(2, "0")}`;
}

export default function Entradas() {
  const [incomes, setIncomes] = useState<Income[] | null>(null);

  const reload = useCallback(() => {
    return invoke<Income[]>("list_incomes")
      .then(setIncomes)
      .catch((err) => toast.error(String(err)));
  }, []);

  useEffect(() => {
    reload();
  }, [reload]);

  async function addIncome(income: NewIncome) {
    await invoke<number>("add_income", income);
    await reload();
  }

  async function deleteIncome(income: Income) {
    try {
      await invoke("delete_income", { id: income.id });
    } catch (err) {
      toast.error(String(err));
      return;
    }
    await reload();
    toast("Entrada removida", {
      description: `${income.description} · ${formatBRL(income.amount_cents)}`,
      action: {
        label: "Desfazer",
        onClick: () => {
          addIncome({
            month: incomeMonth(income),
            description: income.description,
            amountCents: income.amount_cents,
          }).catch((err) => toast.error(String(err)));
        },
      },
    });
  }

  return (
    <div className="flex flex-col gap-4">
      <IncomeForm onSubmit={addIncome} />
      {!incomes ? (
        <LoadingTable />
      ) : incomes.length === 0 ? (
        <EmptyState />
      ) : (
        <IncomesTable incomes={incomes} onDelete={deleteIncome} />
      )}
    </div>
  );
}

function IncomeForm({
  onSubmit,
}: {
  onSubmit: (income: NewIncome) => Promise<void>;
}) {
  const [month, setMonth] = useState(currentMonth);
  const [description, setDescription] = useState("");
  const [amount, setAmount] = useState("");
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  async function handleSubmit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const trimmed = description.trim();
    if (!trimmed) {
      setError("Informe uma descrição.");
      return;
    }
    const amountCents = parseAmountToCents(amount);
    if (amountCents === null) {
      setError("Informe um valor maior que zero, por exemplo 5000,00.");
      return;
    }

    setError("");
    setSaving(true);
    try {
      await onSubmit({ month, description: trimmed, amountCents });
      toast.success(`Entrada de ${formatBRL(amountCents)} adicionada`);
      setMonth(currentMonth());
      setDescription("");
      setAmount("");
    } catch (err) {
      toast.error(String(err));
    } finally {
      setSaving(false);
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Entradas</CardTitle>
        <CardDescription>
          Registre o valor líquido que você recebeu em cada mês.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form
          onSubmit={handleSubmit}
          className="flex flex-col gap-4 sm:flex-row sm:items-end"
          noValidate
        >
          <div className="flex flex-col gap-2 sm:w-48">
            <Label htmlFor="income-month">Mês</Label>
            <Input
              id="income-month"
              type="month"
              value={month}
              onChange={(e) => {
                const value = e.currentTarget.value;
                if (value) setMonth(value);
              }}
            />
          </div>
          <div className="flex flex-1 flex-col gap-2">
            <Label htmlFor="income-description">Descrição</Label>
            <Input
              id="income-description"
              placeholder="Salário, freela…"
              value={description}
              onChange={(e) => setDescription(e.currentTarget.value)}
            />
          </div>
          <div className="flex flex-col gap-2 sm:w-40">
            <Label htmlFor="income-amount">Valor (R$)</Label>
            <Input
              id="income-amount"
              inputMode="decimal"
              placeholder="0,00"
              className="tabular-nums"
              value={amount}
              onChange={(e) => setAmount(e.currentTarget.value)}
            />
          </div>
          <Button type="submit" disabled={saving}>
            {saving ? "Salvando…" : "Adicionar"}
          </Button>
        </form>
        {error && <p className="mt-3 text-sm text-destructive">{error}</p>}
      </CardContent>
    </Card>
  );
}

function LoadingTable() {
  return (
    <Card>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              {COLUMNS.map((c, i) => (
                <TableHead key={i}>{c}</TableHead>
              ))}
            </TableRow>
          </TableHeader>
          <TableBody>
            {Array.from({ length: 3 }, (_, i) => (
              <TableRow key={i}>
                {COLUMNS.map((_, j) => (
                  <TableCell key={j}>
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

function EmptyState() {
  return (
    <Card>
      <CardContent className="flex flex-col items-center gap-3 py-10 text-center">
        <BanknoteArrowUpIcon className="size-10 text-muted-foreground" />
        <div className="flex flex-col gap-1">
          <p className="font-medium">Nenhuma entrada registrada</p>
          <p className="text-sm text-muted-foreground">
            Use o formulário acima para adicionar o que você recebeu.
          </p>
        </div>
      </CardContent>
    </Card>
  );
}

function IncomesTable({
  incomes,
  onDelete,
}: {
  incomes: Income[];
  onDelete: (income: Income) => void;
}) {
  const total = incomes.reduce((sum, i) => sum + i.amount_cents, 0);

  return (
    <Card>
      <CardHeader>
        <CardDescription>Total de entradas</CardDescription>
        <CardTitle className="text-2xl tabular-nums">
          {formatBRL(total)}
        </CardTitle>
      </CardHeader>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Mês</TableHead>
              <TableHead>Descrição</TableHead>
              <TableHead className="text-right">Valor</TableHead>
              <TableHead className="w-12">
                <span className="sr-only">Ações</span>
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {incomes.map((i) => (
              <TableRow key={i.id}>
                <TableCell className="capitalize">
                  {monthLabel(incomeMonth(i))}
                </TableCell>
                <TableCell>{i.description}</TableCell>
                <TableCell className="text-right tabular-nums">
                  {formatBRL(i.amount_cents)}
                </TableCell>
                <TableCell>
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label={`Remover ${i.description}`}
                    onClick={() => onDelete(i)}
                  >
                    <Trash2Icon />
                  </Button>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}
