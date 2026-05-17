import { useState } from "react";
import { useI18n } from "../hooks/useI18n";

interface LockScreenProps {
  initialized: boolean;
  onSetup: (password: string) => Promise<void>;
  onUnlock: (password: string) => Promise<boolean>;
  onResetVault: () => Promise<void>;
}

export default function LockScreen({
  initialized,
  onSetup,
  onUnlock,
  onResetVault,
}: LockScreenProps) {
  const { t } = useI18n();
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError("");
    setLoading(true);

    try {
      if (!initialized) {
        if (password.length < 8) {
          setError(t("passwordTooShort"));
          setLoading(false);
          return;
        }
        if (password !== confirm) {
          setError(t("passwordMismatch"));
          setLoading(false);
          return;
        }
        await onSetup(password);
      } else {
        const ok = await onUnlock(password);
        if (!ok) {
          setError(t("incorrectPassword"));
        }
      }
    } catch (err) {
      setError(String(err));
    }
    setLoading(false);
  };

  const handleResetVault = async () => {
    if (!window.confirm(t("resetVaultConfirm"))) {
      return;
    }

    const verifyText = window.prompt(t("resetVaultTypeResetPrompt")) ?? "";
    if (verifyText.trim().toUpperCase() !== "RESET") {
      setError(t("resetVaultTypeResetMismatch"));
      return;
    }

    setError("");
    setLoading(true);
    try {
      await onResetVault();
      setPassword("");
      setConfirm("");
    } catch (err) {
      setError(String(err));
    }
    setLoading(false);
  };

  return (
    <div className="lock-screen">
      <div className="lock-card">
        <div className="lock-icon">
          <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
            <path d="M7 11V7a5 5 0 0 1 10 0v4" />
          </svg>
        </div>
        <h1>{initialized ? t("unlockVault") : t("createMasterPassword")}</h1>
        <p className="lock-subtitle">
          {initialized
            ? t("enterMasterPassword")
            : t("setMasterPassword")}
        </p>
        <form onSubmit={handleSubmit}>
          <input
            type="password"
            placeholder={t("masterPassword")}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoFocus
          />
          {!initialized && (
            <input
              type="password"
              placeholder={t("confirmPassword")}
              value={confirm}
              onChange={(e) => setConfirm(e.target.value)}
            />
          )}
          {error && <div className="error-msg">{error}</div>}
          <button type="submit" className="btn-primary" disabled={loading}>
            {loading ? "..." : initialized ? t("unlock") : t("createVault")}
          </button>
          {initialized && (
            <button
              type="button"
              className="btn-text"
              onClick={handleResetVault}
              disabled={loading}
            >
              {t("forgotMasterPassword")}
            </button>
          )}
        </form>
      </div>
    </div>
  );
}
