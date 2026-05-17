import { useState, useEffect } from "react";
import { useI18n } from "../hooks/useI18n";
import { useClipboard } from "../hooks/useClipboard";
import { useFolders, flattenTree } from "../hooks/useFolders";
import { PasswordGeneratorPanel } from "./PasswordGeneratorPanel";
import { UsernameGeneratorPanel } from "./UsernameGeneratorPanel";
import { StrengthBadge, scorePassword } from "./StrengthBadge";
import type { VaultEntry, EntryType } from "../types";

interface VaultDetailProps {
  entry: VaultEntry | null;
  isNew: boolean;
  onSave: (entry: VaultEntry) => Promise<void>;
  onDelete: (id: string) => Promise<void>;
  onCancel: () => void;
}

const emptyEntry: VaultEntry = {
  id: "",
  entryType: "login",
  name: "",
  username: "",
  password: "",
  url: "",
  apiKey: "",
  notes: "",
  folder: "",
  favorite: false,
  createdAt: 0,
  updatedAt: 0,
};

export default function VaultDetail({
  entry,
  isNew,
  onSave,
  onDelete,
  onCancel,
}: VaultDetailProps) {
  const { t } = useI18n();
  const { tree } = useFolders();
  const [form, setForm] = useState<VaultEntry>(entry || { ...emptyEntry });
  const [showPassword, setShowPassword] = useState(false);
  const [saving, setSaving] = useState(false);
  const [genPasswordOpen, setGenPasswordOpen] = useState(false);
  const [genUsernameOpen, setGenUsernameOpen] = useState(false);
  const { copied, copy } = useClipboard();

  useEffect(() => {
    setForm(entry || { ...emptyEntry });
    setShowPassword(false);
    setGenPasswordOpen(false);
    setGenUsernameOpen(false);
  }, [entry, isNew]);

  const update = (field: keyof VaultEntry, value: any) => {
    setForm((prev) => ({ ...prev, [field]: value }));
  };

  const handleSave = async () => {
    if (!form.name.trim()) return;
    setSaving(true);
    try {
      await onSave(form);
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async () => {
    if (form.id && confirm(t("deleteEntry"))) {
      await onDelete(form.id);
      onCancel();
    }
  };

  const flattenedFolders = flattenTree(tree);

  if (!entry && !isNew) {
    return (
      <div className="vault-detail empty-detail">
        <div className="empty-detail-inner">
          <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1" opacity="0.3">
            <path d="M12 2L2 7l10 5 10-5-10-5z" />
            <path d="M2 17l10 5 10-5" />
            <path d="M2 12l10 5 10-5" />
          </svg>
          <p>{t("selectEntry")}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="vault-detail">
      <div className="detail-header">
        <h2>{isNew ? t("newEntry") : t("editEntry")}</h2>
        <div className="detail-actions">
          <button
            className={`btn-icon fav-btn ${form.favorite ? "active" : ""}`}
            onClick={() => update("favorite", !form.favorite)}
            title={t("toggleFavorite")}
          >
            <svg width="18" height="18" viewBox="0 0 24 24" fill={form.favorite ? "currentColor" : "none"} stroke="currentColor" strokeWidth="1.5">
              <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
            </svg>
          </button>
          {!isNew && (
            <button className="btn-icon delete-btn" onClick={handleDelete} title={t("delete")}>
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
                <polyline points="3 6 5 6 21 6" />
                <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
              </svg>
            </button>
          )}
        </div>
      </div>

      <div className="detail-body">
        <div className="field-group">
          <label>{t("type")}</label>
          <div className="type-toggle">
            <button
              className={`type-btn ${form.entryType === "login" ? "active" : ""}`}
              onClick={() => update("entryType", "login" as EntryType)}
            >
              {t("login")}
            </button>
            <button
              className={`type-btn ${form.entryType === "apiKey" ? "active" : ""}`}
              onClick={() => update("entryType", "apiKey" as EntryType)}
            >
              {t("apiKey")}
            </button>
          </div>
        </div>

        <div className="field-group">
          <label>{t("name")} *</label>
          <input
            type="text"
            value={form.name}
            onChange={(e) => update("name", e.target.value)}
            placeholder={t("examplePrefix") + "GitHub"}
          />
        </div>

        {form.entryType === "login" && (
          <>
            <div className="field-group">
              <label>{t("usernameEmail")}</label>
              <div className="field-with-action">
                <input
                  type="text"
                  value={form.username || ""}
                  onChange={(e) => update("username", e.target.value)}
                  placeholder="user@example.com"
                />
                <button className="btn-generate" onClick={() => setGenUsernameOpen(!genUsernameOpen)} title={t("generate")}>
                  ⚡
                </button>
                {form.username && (
                  <button className="btn-copy" onClick={() => copy(form.username!)}>
                    {copied ? t("copied") : t("copy")}
                  </button>
                )}
              </div>
              {genUsernameOpen && (
                <UsernameGeneratorPanel
                  onApply={(username) => { update("username", username); setGenUsernameOpen(false); }}
                  onClose={() => setGenUsernameOpen(false)}
                />
              )}
            </div>

            <div className="field-group">
              <label>{t("password")}</label>
              <div className="field-with-action">
                <input
                  type={showPassword ? "text" : "password"}
                  value={form.password || ""}
                  onChange={(e) => update("password", e.target.value)}
                  placeholder={t("password")}
                />
                <button className="btn-icon" onClick={() => setShowPassword(!showPassword)}>
                  {showPassword ? t("hide") : t("show")}
                </button>
                <button className="btn-generate" onClick={() => setGenPasswordOpen(!genPasswordOpen)} title={t("generate")}>
                  ⚡
                </button>
                {form.password && (
                  <button className="btn-copy" onClick={() => copy(form.password!)}>
                    {copied ? t("copied") : t("copy")}
                  </button>
                )}
              </div>
              <StrengthBadge level={scorePassword(form.password ?? "")} />
              {genPasswordOpen && (
                <PasswordGeneratorPanel
                  onApply={(password) => { update("password", password); setGenPasswordOpen(false); }}
                  onClose={() => setGenPasswordOpen(false)}
                />
              )}
            </div>

            <div className="field-group">
              <label>{t("url")}</label>
              <input
                type="text"
                value={form.url || ""}
                onChange={(e) => update("url", e.target.value)}
                placeholder="https://example.com"
              />
            </div>
          </>
        )}

        {form.entryType === "apiKey" && (
          <>
            <div className="field-group">
              <label>{t("apiKeyField")}</label>
              <div className="field-with-action">
                <input
                  type={showPassword ? "text" : "password"}
                  value={form.apiKey || ""}
                  onChange={(e) => update("apiKey", e.target.value)}
                  placeholder="sk-..."
                />
                <button className="btn-icon" onClick={() => setShowPassword(!showPassword)}>
                  {showPassword ? t("hide") : t("show")}
                </button>
                <button className="btn-generate" onClick={() => setGenPasswordOpen(!genPasswordOpen)} title={t("generate")}>
                  ⚡
                </button>
                {form.apiKey && (
                  <button className="btn-copy" onClick={() => copy(form.apiKey!)}>
                    {copied ? t("copied") : t("copy")}
                  </button>
                )}
              </div>
              <StrengthBadge level={scorePassword(form.apiKey ?? "")} />
              {genPasswordOpen && (
                <PasswordGeneratorPanel
                  onApply={(apiKey) => { update("apiKey", apiKey); setGenPasswordOpen(false); }}
                  onClose={() => setGenPasswordOpen(false)}
                />
              )}
            </div>

            <div className="field-group">
              <label>{t("urlService")}</label>
              <input
                type="text"
                value={form.url || ""}
                onChange={(e) => update("url", e.target.value)}
                placeholder="https://api.example.com"
              />
            </div>
          </>
        )}

        <div className="field-group">
          <label>{t("folder")}</label>
          <select
            value={form.folder || ""}
            onChange={(e) => update("folder", e.target.value || null)}
          >
            <option value="">{t("noFolder")}</option>
            {flattenedFolders.map((n) => (
              <option key={n.id} value={n.id}>{"\u00a0\u00a0".repeat(n.depth)}{n.name}</option>
            ))}
          </select>
        </div>

        <div className="field-group">
          <label>{t("notes")}</label>
          <textarea
            value={form.notes || ""}
            onChange={(e) => update("notes", e.target.value)}
            placeholder={t("addNotes")}
            rows={3}
          />
        </div>
      </div>

      <div className="detail-footer">
        <button className="btn-secondary" onClick={onCancel}>{t("cancel")}</button>
        <button className="btn-primary" onClick={handleSave} disabled={saving || !form.name.trim()}>
          {saving ? t("saving") : t("save")}
        </button>
      </div>
    </div>
  );
}
