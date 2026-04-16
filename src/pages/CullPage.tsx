import { useEffect } from "react";
import { useParams, useNavigate } from "react-router-dom";
import { useProjectStore } from "../stores/projectStore";
import { useKeyboardNav } from "../hooks/useKeyboardNav";
import { LoupeView } from "../components/LoupeView";
import { Filmstrip } from "../components/Filmstrip";
import { Toolbar } from "../components/Toolbar";
import { RatingBar } from "../components/RatingBar";
import { MetadataOverlay } from "../components/MetadataOverlay";
import { ShortcutHints } from "../components/ShortcutHints";

export function CullPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { currentShoot, isLoading, loadShoot } = useProjectStore();
  useKeyboardNav();

  useEffect(() => {
    const shootId = Number(id);
    if (!isNaN(shootId) && shootId > 0) {
      loadShoot(shootId);
    } else {
      navigate("/shoots", { replace: true });
    }
  }, [id, loadShoot, navigate]);

  if (isLoading) {
    return (
      <div className="h-screen w-screen flex items-center justify-center bg-[var(--bg-primary)]">
        <p className="text-[var(--text-secondary)]">Loading shoot...</p>
      </div>
    );
  }

  if (!currentShoot) {
    return (
      <div className="h-screen w-screen flex items-center justify-center bg-[var(--bg-primary)]">
        <p className="text-[var(--text-secondary)]">Shoot not found</p>
      </div>
    );
  }

  return (
    <div className="h-screen w-screen flex flex-col bg-[var(--bg-primary)]">
      <Toolbar />
      <div className="flex-1 relative overflow-hidden">
        <LoupeView />
        <MetadataOverlay />
        <ShortcutHints />
      </div>
      <Filmstrip />
      <RatingBar />
    </div>
  );
}
