function App() {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        height: "100vh",
        flexDirection: "column",
        gap: "1rem",
      }}
    >
      <h1 style={{ fontSize: "2rem", fontWeight: 600 }}>PhotoSift</h1>
      <p style={{ color: "var(--text-secondary)" }}>
        Drop a folder or press <kbd>Ctrl+O</kbd> to open
      </p>
    </div>
  );
}

export default App;
