import { Button } from "@/components/ui/button";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
} from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";
import { currentMonth, formatBRL } from "@/lib/utils";
import { useState, FormEvent } from "react";
import { toast } from "sonner";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

const categories = [
  { label: "CDB, Renda Fixa e Estruturados", value: "cdb" },
  { label: "Fundos de investimento", value: "Fundos" },
];

function Investimentos() {
  function handleSubmit(income: NewInvestment) {
    console.log(income);
    return new Promise(() => {}) as Promise<void>;
  }

  return (
    <div>
      <IncomeForm onSubmit={handleSubmit} />
    </div>
  );
}

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

type NewInvestment = { month: string; category: string; amountCents: number };

function IncomeForm({
  onSubmit,
}: {
  onSubmit: (income: NewInvestment) => Promise<void>;
}) {
  const [month, setMonth] = useState(currentMonth);
  const [category, setCategory] = useState("");
  const [amount, setAmount] = useState("");
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  async function handleSubmit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();

    const amountCents = parseAmountToCents(amount);
    if (amountCents === null) {
      setError("Informe um valor maior que zero, por exemplo 5000,00.");
      return;
    }

    setError("");
    setSaving(true);
    try {
      await onSubmit({ month, category: category, amountCents });
      toast.success(`Entrada de ${formatBRL(amountCents)} adicionada`);
      setMonth(currentMonth());
      setCategory("");
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
        <CardTitle>Investimento</CardTitle>
        <CardDescription>
          Registre o valor total dos seus investimentos naquele mês. Isso não é
          o valor de aporte e sim o total investido
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
            <Label htmlFor="income-description">Categoria</Label>
            <Select
              defaultValue="cdb"
              value={category}
              onValueChange={(e) => {
                setCategory(e);
              }}
            >
              <SelectTrigger className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectLabel>Fruits</SelectLabel>
                  {categories.map((item) => (
                    <SelectItem key={item.value} value={item.value}>
                      {item.label}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
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

export default Investimentos;
