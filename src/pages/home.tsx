import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
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

export default function Home() {
  const [greetMsg, setGreetMsg] = useState("");
  const [name, setName] = useState("");

  async function greet() {
    // Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
    setGreetMsg(await invoke("greet", { name }));
  }

  return (
    <div className="flex justify-center">
      <Card className="w-full max-w-sm">
        <CardHeader>
          <CardTitle>facaopix</CardTitle>
          <CardDescription>Tauri + React + shadcn/ui</CardDescription>
        </CardHeader>
        <CardContent>
          <form
            className="flex flex-col gap-3"
            onSubmit={(e) => {
              e.preventDefault();
              greet();
            }}
          >
            <Label htmlFor="greet-input">Name</Label>
            <div className="flex gap-2">
              <Input
                id="greet-input"
                onChange={(e) => setName(e.currentTarget.value)}
                placeholder="Enter a name..."
              />
              <Button type="submit">Greet</Button>
            </div>
            {greetMsg && (
              <p className="text-sm text-muted-foreground">{greetMsg}</p>
            )}
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
