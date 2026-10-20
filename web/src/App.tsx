import { Link, Route, Routes, useParams } from "react-router";

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
        <Routes>
          <Route path="/" element={<ShootsPage />} />
          <Route path="/import/:scanId" element={<ImportPlaceholder />} />
          <Route path="/review/:shootId" element={<ReviewPlaceholder />} />
          <Route path="*" element={<p className="muted">Page not found.</p>} />
        </Routes>
      </main>
    </div>
  );
}

// The import and review screens arrive in later tasks; the routes exist now so
// links from the shoot list and the card banner already resolve.
function ImportPlaceholder() {
  const { scanId } = useParams();
  return <p className="muted">Import {scanId}: coming soon.</p>;
}

function ReviewPlaceholder() {
  const { shootId } = useParams();
  return <p className="muted">Review of shoot {shootId}: coming soon.</p>;
}
