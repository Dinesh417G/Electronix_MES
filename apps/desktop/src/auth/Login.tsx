// Dual login: a big PIN pad for kiosk operators and a password form for the
// supervisor console. Both mint the same bearer token (§12 M1).

import { useState } from "react";
import { useAuth } from "./AuthContext";
import { ErrorNote } from "../components/ui";

type Mode = "pin" | "password";

export function Login() {
  const { pinLogin, passwordLogin } = useAuth();
  const [mode, setMode] = useState<Mode>("pin");
  const [username, setUsername] = useState("");
  const [pin, setPin] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);

  async function submit() {
    setBusy(true);
    setError(null);
    try {
      if (mode === "pin") await pinLogin(username, pin);
      else await passwordLogin(username, password);
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex min-h-screen items-center justify-center bg-slate-100">
      <div className="w-full max-w-sm rounded-2xl bg-white p-8 shadow-lg">
        <div className="mb-6 text-center">
          <h1 className="text-2xl font-bold text-slate-800">ElectronIx MES</h1>
          <p className="text-sm text-slate-500">
            {mode === "pin" ? "Operator kiosk sign-in" : "Supervisor console sign-in"}
          </p>
        </div>

        <input
          className="mb-3 w-full rounded-lg border border-slate-300 px-3 py-2"
          placeholder="Username"
          value={username}
          onChange={(e) => setUsername(e.target.value)}
        />

        {mode === "pin" ? (
          <PinPad pin={pin} onChange={setPin} />
        ) : (
          <input
            className="mb-3 w-full rounded-lg border border-slate-300 px-3 py-2"
            type="password"
            placeholder="Password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && submit()}
          />
        )}

        {error != null && (
          <div className="mb-3">
            <ErrorNote error={error} />
          </div>
        )}

        <button
          className="mb-3 w-full rounded-lg bg-blue-600 py-3 font-semibold text-white disabled:opacity-50"
          onClick={submit}
          disabled={busy || !username}
        >
          {busy ? "Signing in…" : "Sign in"}
        </button>

        <button
          className="w-full text-sm text-slate-500 underline"
          onClick={() => {
            setMode(mode === "pin" ? "password" : "pin");
            setError(null);
          }}
        >
          {mode === "pin" ? "Use password (supervisor)" : "Use PIN (kiosk)"}
        </button>
      </div>
    </div>
  );
}

function PinPad({ pin, onChange }: { pin: string; onChange: (v: string) => void }) {
  const keys = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "clear", "0", "del"];
  return (
    <div className="mb-3">
      <div className="mb-3 h-10 rounded-lg border border-slate-300 text-center text-2xl tracking-[0.5em]">
        {"•".repeat(pin.length)}
      </div>
      <div className="grid grid-cols-3 gap-2">
        {keys.map((k) => (
          <button
            key={k}
            className="rounded-lg bg-slate-100 py-4 text-lg font-semibold text-slate-700 active:bg-slate-200"
            onClick={() => {
              if (k === "clear") onChange("");
              else if (k === "del") onChange(pin.slice(0, -1));
              else onChange(pin + k);
            }}
          >
            {k === "del" ? "⌫" : k === "clear" ? "C" : k}
          </button>
        ))}
      </div>
    </div>
  );
}
