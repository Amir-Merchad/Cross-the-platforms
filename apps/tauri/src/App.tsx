import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

// Minimal status view; the full settings UI replaces this (plan Task 7).
export default function App() {
  const [status, setStatus] = useState<unknown>(null);
  useEffect(() => {
    invoke("get_status").then(setStatus);
    const off = listen("status", (e) => setStatus(e.payload));
    return () => { off.then((f) => f()); };
  }, []);
  return (
    <main className="p-6 font-sans">
      <h1 className="text-xl font-semibold">Cross the Platforms</h1>
      <pre className="mt-4 text-xs">{JSON.stringify(status, null, 2)}</pre>
    </main>
  );
}
