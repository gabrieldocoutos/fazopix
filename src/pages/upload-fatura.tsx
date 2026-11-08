import { useState, type DragEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FileTextIcon, UploadIcon, XIcon } from "lucide-react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
} from "@/components/ui/card";

type Status = "idle" | "sending" | "error";

function isPdf(file: File) {
  return (
    file.type === "application/pdf" || file.name.toLowerCase().endsWith(".pdf")
  );
}

function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export default function UploadFatura() {
  const [file, setFile] = useState<File | null>(null);
  const [dragging, setDragging] = useState(false);
  const [status, setStatus] = useState<Status>("idle");
  const [error, setError] = useState("");

  function selectFile(selected: File | undefined) {
    if (!selected) return;
    if (!isPdf(selected)) {
      setFile(null);
      setStatus("error");
      setError("Apenas arquivos PDF são aceitos.");
      return;
    }
    setFile(selected);
    setStatus("idle");
    setError("");
  }

  function handleDrop(e: DragEvent<HTMLLabelElement>) {
    e.preventDefault();
    setDragging(false);
    selectFile(e.dataTransfer.files[0]);
  }

  async function handleSubmit() {
    if (!file) return;
    setStatus("sending");
    setError("");
    try {
      await invoke("upload_fatura", new Uint8Array(await file.arrayBuffer()), {
        headers: { "x-file-name": encodeURIComponent(file.name) },
      });
      toast.success("Fatura enviada.", { description: file.name });
      setStatus("idle");
      setFile(null);
    } catch (err) {
      setStatus("error");
      setError(String(err));
    }
  }

  return (
    <div className="flex flex-col gap-4">
      
      <Card className="max-w-lg">
        <CardHeader>
          
          <CardDescription>
            Envie a fatura do cartão de crédito em PDF.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form
            className="flex flex-col gap-4"
            onSubmit={(e) => {
              e.preventDefault();
              handleSubmit();
            }}
          >
            <label
              htmlFor="fatura-input"
              onDragOver={(e) => {
                e.preventDefault();
                setDragging(true);
              }}
              onDragLeave={() => setDragging(false)}
              onDrop={handleDrop}
              className={cn(
                "flex cursor-pointer flex-col items-center justify-center gap-2 rounded-lg border-2 border-dashed px-6 py-10 text-center text-sm text-muted-foreground transition-colors hover:bg-muted/50 [&>*]:pointer-events-none",
                dragging && "border-primary bg-primary/5 text-foreground",
              )}
            >
              <UploadIcon className="size-6" />
              <span>Arraste a fatura aqui ou clique para escolher</span>
              <span className="text-xs">Apenas PDF</span>
            </label>
            <input
              id="fatura-input"
              type="file"
              accept="application/pdf,.pdf"
              className="hidden"
              onChange={(e) => {
                selectFile(e.currentTarget.files?.[0]);
                e.currentTarget.value = "";
              }}
            />

            {file && (
              <div className="flex items-center gap-2 rounded-lg border px-3 py-2 text-sm">
                <FileTextIcon className="size-4 shrink-0 text-muted-foreground" />
                <span className="truncate">{file.name}</span>
                <span className="ml-auto shrink-0 text-xs text-muted-foreground">
                  {formatSize(file.size)}
                </span>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  aria-label="Remover arquivo"
                  onClick={() => setFile(null)}
                >
                  <XIcon />
                </Button>
              </div>
            )}

            {status === "error" && (
              <p className="text-sm text-destructive">{error}</p>
            )}

            <Button
              type="submit"
              className="self-end"
              disabled={!file || status === "sending"}
            >
              {status === "sending" ? "Enviando..." : "Enviar"}
            </Button>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
