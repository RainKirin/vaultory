import { useState, useCallback, useRef } from "react";

export function useClipboard() {
  const [copied, setCopied] = useState(false);
  const lastCopyRef = useRef<string>("");

  const copy = useCallback(async (text: string, clearAfterMs: number = 30000) => {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      const ta = document.createElement("textarea");
      ta.value = text;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      document.body.removeChild(ta);
    }
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
    lastCopyRef.current = text;

    if (clearAfterMs > 0) {
      setTimeout(async () => {
        if (lastCopyRef.current !== text) return;
        try {
          const current = await navigator.clipboard.readText();
          if (current === text) {
            await navigator.clipboard.writeText("");
            lastCopyRef.current = "";
          }
        } catch {
          // 读权限失败静默不清
        }
      }, clearAfterMs);
    }
  }, []);

  return { copied, copy };
}
