import { Link, Route, Routes, useParams } from "react-router";

import { CardBanner } from "./components/CardBanner";
import { ImportPage } from "./pages/ImportPage";
import { ShootsPage } from "./pages/ShootsPage";

export function App() {
  return (
    <div className="app">
      <header className="app-header">
        <Link to="/" className="brand">
          PhotoSift
        </Link>
      </header>
      <main className="app-main">
        <CardBanner />
        <Routes>
          <Route path="/" element={<ShootsPage />} />
          <Route path="/import/:scanId" element={<ImportPage />} />
          <Route path="/review/:shootId" element={<ReviewPlaceholder />} />
          <Route path="*" element={<p className="muted">Page not found.</p>} />
        </Routes>
      </main>
    </div>
  );
}

// The review screen arrives in a later task; the route exists now so links
// from the shoot list and the import result already resolve.
function ReviewPlaceholder() {
  const { shootId } = useParams();
  return <p className="muted">Review of shoot {shootId}: coming soon.</p>;
}
