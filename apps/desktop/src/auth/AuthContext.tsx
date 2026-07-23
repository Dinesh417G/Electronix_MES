// Auth state: holds the current session (token + identity) and the login /
// logout actions. The token is persisted so a kiosk survives a reload.

import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from "react";
import { api, setToken } from "../api/client";
import type { LoginResponse } from "../api/types";

interface Session {
  userId: string;
  username: string;
  role: string;
}

interface AuthState {
  session: Session | null;
  passwordLogin: (username: string, password: string) => Promise<void>;
  pinLogin: (username: string, pin: string) => Promise<void>;
  logout: () => void;
}

const AuthContext = createContext<AuthState | null>(null);

function sessionFrom(r: LoginResponse): Session {
  return { userId: r.user_id, username: r.username, role: r.role_code };
}

export function AuthProvider({ children }: { children: ReactNode }) {
  const [session, setSession] = useState<Session | null>(null);

  const passwordLogin = useCallback(async (username: string, password: string) => {
    const r = await api<LoginResponse>("/v1/auth/login", {
      method: "POST",
      body: { username, password },
      auth: false,
    });
    setToken(r.token);
    setSession(sessionFrom(r));
  }, []);

  const pinLogin = useCallback(async (username: string, pin: string) => {
    const r = await api<LoginResponse>("/v1/auth/pin-login", {
      method: "POST",
      body: { username, pin },
      auth: false,
    });
    setToken(r.token);
    setSession(sessionFrom(r));
  }, []);

  const logout = useCallback(() => {
    setToken(null);
    setSession(null);
  }, []);

  const value = useMemo(
    () => ({ session, passwordLogin, pinLogin, logout }),
    [session, passwordLogin, pinLogin, logout],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthState {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("useAuth must be used within AuthProvider");
  return ctx;
}
