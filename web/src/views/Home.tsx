interface HomeProps {
  busy: boolean;
  onOpen(): void;
}

/** The idle screen: pick a local file to play. Nothing is uploaded. */
export function Home({ busy, onOpen }: HomeProps) {
  return (
    <main className="home">
      <h1>PeerLink</h1>
      <p>
        Open a local MP4. It plays in this window, served from your own machine
        over a private loopback connection — the file never leaves your disk.
      </p>
      <button type="button" onClick={onOpen} disabled={busy}>
        {busy ? "Opening…" : "Open MP4"}
      </button>
    </main>
  );
}
