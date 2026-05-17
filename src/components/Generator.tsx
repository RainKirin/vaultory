import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "../hooks/useI18n";
import { useClipboard } from "../hooks/useClipboard";
import type { PasswordGeneratorConfig, GeneratedPassword, UsernameConfig } from "../types";

const defaultConfig: PasswordGeneratorConfig = {
  length: 20,
  uppercase: true,
  lowercase: true,
  numbers: true,
  symbols: true,
  excludeAmbiguous: false,
};

export default function Generator() {
  const { t } = useI18n();
  const [tab, setTab] = useState<"password" | "username">("password");
  const [config, setConfig] = useState<PasswordGeneratorConfig>(defaultConfig);
  const [result, setResult] = useState<GeneratedPassword | null>(null);
  const [username, setUsername] = useState("");
  const [usernameFormat, setUsernameFormat] = useState("word1234");
  const { copied, copy } = useClipboard();

  const generate = useCallback(async () => {
    const r = await invoke<GeneratedPassword>("generate_password_cmd", { config });
    setResult(r);
  }, [config]);

  const generateUser = useCallback(async () => {
    const u = await invoke<string>("generate_username_cmd", {
      config: { format: usernameFormat } as UsernameConfig,
    });
    setUsername(u);
  }, [usernameFormat]);

  useEffect(() => {
    if (tab === "password") generate();
    else generateUser();
  }, [tab, generate, generateUser]);

  const updateConfig = (key: keyof PasswordGeneratorConfig, value: any) => {
    setConfig((prev) => ({ ...prev, [key]: value }));
  };

  const strengthColor = (s: string) => {
    if (s === "weak") return "#ff3b30";
    if (s === "medium") return "#ff9500";
    return "#34c759";
  };

  const strengthLabel = (s: string) => {
    if (s === "weak") return t("weak");
    if (s === "medium") return t("medium");
    return t("strong");
  };

  return (
    <div className="generator">
      <h2>{t("generator")}</h2>
      <div className="gen-tabs">
        <button
          className={`gen-tab ${tab === "password" ? "active" : ""}`}
          onClick={() => setTab("password")}
        >
          {t("passwordTab")}
        </button>
        <button
          className={`gen-tab ${tab === "username" ? "active" : ""}`}
          onClick={() => setTab("username")}
        >
          {t("usernameTab")}
        </button>
      </div>

      {tab === "password" && (
        <div className="gen-panel">
          <div className="gen-result">
            <code className="gen-output">{result?.password || "..."}</code>
            <div className="gen-result-actions">
              <button className="btn-copy" onClick={() => result && copy(result.password)}>
                {copied ? t("copied") : t("copy")}
              </button>
              <button className="btn-icon" onClick={generate} title={t("regenerate")}>
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <polyline points="23 4 23 10 17 10" />
                  <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" />
                </svg>
              </button>
            </div>
          </div>

          {result && (
            <div className="strength-bar">
              <div
                className="strength-fill"
                style={{
                  width: result.strength === "weak" ? "33%" : result.strength === "medium" ? "66%" : "100%",
                  backgroundColor: strengthColor(result.strength),
                }}
              />
              <span style={{ color: strengthColor(result.strength) }}>
                {strengthLabel(result.strength)}
              </span>
            </div>
          )}

          <div className="field-group">
            <label>{t("length")}: {config.length}</label>
            <input
              type="range"
              min={8}
              max={128}
              value={config.length}
              onChange={(e) => updateConfig("length", parseInt(e.target.value))}
            />
          </div>

          <div className="gen-checkboxes">
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={config.uppercase}
                onChange={(e) => updateConfig("uppercase", e.target.checked)}
              />
              {t("uppercase")}
            </label>
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={config.lowercase}
                onChange={(e) => updateConfig("lowercase", e.target.checked)}
              />
              {t("lowercase")}
            </label>
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={config.numbers}
                onChange={(e) => updateConfig("numbers", e.target.checked)}
              />
              {t("numbers")}
            </label>
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={config.symbols}
                onChange={(e) => updateConfig("symbols", e.target.checked)}
              />
              {t("symbols")}
            </label>
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={config.excludeAmbiguous}
                onChange={(e) => updateConfig("excludeAmbiguous", e.target.checked)}
              />
              {t("excludeAmbiguous")}
            </label>
          </div>
        </div>
      )}

      {tab === "username" && (
        <div className="gen-panel">
          <div className="gen-result">
            <code className="gen-output">{username || "..."}</code>
            <div className="gen-result-actions">
              <button className="btn-copy" onClick={() => username && copy(username)}>
                {copied ? t("copied") : t("copy")}
              </button>
              <button className="btn-icon" onClick={generateUser} title={t("regenerate")}>
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <polyline points="23 4 23 10 17 10" />
                  <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" />
                </svg>
              </button>
            </div>
          </div>

          <div className="field-group">
            <label>{t("format")}</label>
            <div className="format-options">
              {["word1234", "word_word", "word.word"].map((fmt) => (
                <button
                  key={fmt}
                  className={`format-btn ${usernameFormat === fmt ? "active" : ""}`}
                  onClick={() => setUsernameFormat(fmt)}
                >
                  {fmt}
                </button>
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
