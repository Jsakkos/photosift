import { Link, Route, Routes, useLocation } from "react-router";

import { CardBanner } from "./components/CardBanner";
import { ImportPage } from "./pages/ImportPage";
import { ReviewPage } from "./pages/ReviewPage";
import { ShootsPage } from "./pages/ShootsPage";
import { TastePage } from "./pages/TastePage";

export function App() {
  // The review screen uses the whole window: photos are the content.
  const wide = useLocation().pathname.startsWith("/review/");
  return (
    <div className="app">
      <header className="app-header">
        <Link to="/" className="brand">
          PhotoSift
        </Link>
      </header>
      <main className={wide ? "app-main app-main-wide" : "app-main"}>
        <CardBanner />
        <Routes>
          <Route path="/" element={<ShootsPage />} />
          <Route path="/import/:scanId" element={<ImportPage />} />
          <Route path="/review/:shootId" element={<ReviewPage />} />
          <Route path="/taste" element={<TastePage />} />
          <Route path="*" element={<p className="muted">Page not found.</p>} />
        </Routes>
      </main>
    </div>
  );
}
