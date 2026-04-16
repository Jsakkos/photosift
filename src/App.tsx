import { Routes, Route, Navigate } from "react-router-dom";
import { ShootListPage } from "./pages/ShootListPage";
import { CullPage } from "./pages/CullPage";

function App() {
  return (
    <Routes>
      <Route path="/" element={<Navigate to="/shoots" replace />} />
      <Route path="/shoots" element={<ShootListPage />} />
      <Route path="/shoots/:id" element={<CullPage />} />
    </Routes>
  );
}

export default App;
