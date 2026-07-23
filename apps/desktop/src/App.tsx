// Top-level routing. Operators (kiosk) and supervisors/planners (console) share
// one binary; the landing route is chosen by role after login. A supervisor can
// still open the kiosk, but an operator is kept out of the console.

import { Navigate, Route, Routes } from "react-router-dom";
import { useAuth } from "./auth/AuthContext";
import { Login } from "./auth/Login";
import { Kiosk } from "./kiosk/Kiosk";
import { Supervisor } from "./supervisor/Supervisor";

const CONSOLE_ROLES = ["Admin", "Planner", "Supervisor", "Quality", "Maintenance"];

export function App() {
  const { session } = useAuth();

  if (!session) return <Login />;

  const canConsole = CONSOLE_ROLES.includes(session.role);
  const home = canConsole ? "/supervisor" : "/kiosk";

  return (
    <Routes>
      <Route path="/kiosk" element={<Kiosk />} />
      <Route
        path="/supervisor/*"
        element={canConsole ? <Supervisor /> : <Navigate to="/kiosk" replace />}
      />
      <Route path="*" element={<Navigate to={home} replace />} />
    </Routes>
  );
}
