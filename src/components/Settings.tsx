import { useState, useEffect } from "react";
import { useI18n } from "../hooks/useI18n";
import { useFolders } from "../hooks/useFolders";
import type { AppSettings, Folder, FolderNode, Language } from "../types";

interface SettingsProps {
  settings: AppSettings;
  folders: Folder[];
  onSaveSettings: (s: AppSettings) => Promise<void>;
  onSaveFolder: (f: Folder) => Promise<void>;
  onDeleteFolder: (id: string, strategy: string) => Promise<void>;
  onChangeMasterPassword: (currentPassword: string, newPassword: string) => Promise<void>;
}

export default function Settings({
  settings,
  folders,
  onSaveSettings,
  onSaveFolder,
  onDeleteFolder,
  onChangeMasterPassword,
}: SettingsProps) {
  const { t, lang, setLang } = useI18n();
  const { tree } = useFolders(folders);
  const [local, setLocal] = useState(settings);
  const [newFolder, setNewFolder] = useState("");
  const [saved, setSaved] = useState(false);
  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [confirmNewPassword, setConfirmNewPassword] = useState("");
  const [changingPassword, setChangingPassword] = useState(false);
  const [passwordChanged, setPasswordChanged] = useState(false);
  const [passwordError, setPasswordError] = useState("");

  useEffect(() => {
    setLocal(settings);
  }, [settings]);

  const handleSave = async () => {
    await onSaveSettings(local);
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  const handleAddFolder = async (parentId: string | null = null) => {
    const name =
      parentId !== null
        ? window.prompt(t("enterSubfolderName")) ?? ""
        : newFolder.trim();

    if (!name.trim()) {
      return;
    }

    try {
      await onSaveFolder({
        id: crypto.randomUUID(),
        name: name.trim(),
        parentId,
        sortOrder: Date.now(),
      });
      if (!parentId) {
        setNewFolder("");
      }
    } catch {
      // 错误已由 App 的 lastError 集中处理
    }
  };

  const handleDeleteFolder = async (id: string) => {
    const hasChildren = folders.some((folder) => folder.parentId === id);
    let strategy = "merge_up";

    if (hasChildren) {
      const cascadeSelected = window.confirm(t("cascadeFolderPrompt"));
      strategy = cascadeSelected ? "cascade" : "merge_up";
    }

    const confirmed = window.confirm(
      strategy === "cascade" ? t("confirmCascadeDelete") : t("deleteFolder"),
    );

    if (!confirmed) {
      return;
    }

    try {
      await onDeleteFolder(id, strategy);
    } catch {
      // 错误已由 App 的 lastError 集中处理
    }
  };

  const handleLanguageChange = (l: Language) => {
    setLang(l);
    setLocal({ ...local, language: l });
  };

  const handleChangeMasterPassword = async () => {
    setPasswordError("");
    setPasswordChanged(false);

    if (newPassword.length < 8) {
      setPasswordError(t("passwordTooShort"));
      return;
    }
    if (newPassword !== confirmNewPassword) {
      setPasswordError(t("passwordMismatch"));
      return;
    }

    setChangingPassword(true);
    try {
      await onChangeMasterPassword(currentPassword, newPassword);
      setCurrentPassword("");
      setNewPassword("");
      setConfirmNewPassword("");
      setPasswordChanged(true);
      setTimeout(() => setPasswordChanged(false), 3000);
    } catch (e) {
      setPasswordError(String(e));
    }
    setChangingPassword(false);
  };

  return (
    <div className="settings">
      <h2>{t("settings")}</h2>

      <div className="settings-section">
        <h3>{t("language")}</h3>
        <div className="type-toggle" style={{ maxWidth: 200 }}>
          <button
            className={`type-btn ${lang === "zh" ? "active" : ""}`}
            onClick={() => handleLanguageChange("zh")}
          >
            中文
          </button>
          <button
            className={`type-btn ${lang === "en" ? "active" : ""}`}
            onClick={() => handleLanguageChange("en")}
          >
            English
          </button>
        </div>
      </div>

      <div className="settings-section">
        <h3>{t("security")}</h3>
        <div className="field-group">
          <label>{t("autoLock")}</label>
          <select
            value={local.autoLockMinutes}
            onChange={(e) =>
              setLocal({
                ...local,
                autoLockMinutes: parseInt(e.target.value, 10),
              })
            }
          >
            <option value={1}>1 {t("minutes")}</option>
            <option value={5}>5 {t("minutes")}</option>
            <option value={15}>15 {t("minutes")}</option>
            <option value={30}>30 {t("minutes")}</option>
          </select>
        </div>
        <div className="settings-password-form">
          <div className="field-group">
            <label>{t("currentMasterPassword")}</label>
            <input
              type="password"
              value={currentPassword}
              onChange={(e) => setCurrentPassword(e.target.value)}
              placeholder={t("currentMasterPassword")}
            />
          </div>
          <div className="field-group">
            <label>{t("newMasterPassword")}</label>
            <input
              type="password"
              value={newPassword}
              onChange={(e) => setNewPassword(e.target.value)}
              placeholder={t("newMasterPassword")}
            />
          </div>
          <div className="field-group">
            <label>{t("confirmNewPassword")}</label>
            <input
              type="password"
              value={confirmNewPassword}
              onChange={(e) => setConfirmNewPassword(e.target.value)}
              placeholder={t("confirmNewPassword")}
            />
          </div>
          {passwordError && <div className="error-msg">{passwordError}</div>}
          <div className="settings-password-actions">
            <button
              type="button"
              className="btn-secondary"
              onClick={handleChangeMasterPassword}
              disabled={changingPassword}
            >
              {changingPassword ? t("saving") : t("changeMasterPassword")}
            </button>
            {passwordChanged && <span className="text-muted">{t("masterPasswordChanged")}</span>}
          </div>
        </div>
      </div>

      <div className="settings-section">
        <h3>{t("appearance")}</h3>
        <label className="checkbox-label">
          <input
            type="checkbox"
            checked={local.darkMode}
            onChange={(e) =>
              setLocal({ ...local, darkMode: e.target.checked })
            }
          />
          {t("darkMode")}
        </label>
      </div>

      <div className="settings-section">
        <h3>{t("folders")}</h3>
        <div className="folder-add">
          <input
            type="text"
            value={newFolder}
            onChange={(e) => setNewFolder(e.target.value)}
            placeholder={t("newFolder")}
            onKeyDown={(e) => e.key === "Enter" && handleAddFolder()}
          />
          <button
            className="btn-primary"
            onClick={() => handleAddFolder(null)}
          >
            {t("addFolder")}
          </button>
        </div>
        <div className="folder-list">
          {tree.map((node) => (
            <FolderTreeRow
              key={node.id}
              node={node}
              onAddChild={handleAddFolder}
              onDelete={handleDeleteFolder}
            />
          ))}
          {tree.length === 0 && (
            <p className="text-muted">{t("noFolders")}</p>
          )}
        </div>
      </div>

      <div className="settings-footer">
        <button className="btn-primary" onClick={handleSave}>
          {saved ? t("saved") : t("saveSettings")}
        </button>
      </div>
    </div>
  );
}

interface FolderTreeRowProps {
  node: FolderNode;
  onAddChild: (parentId: string) => Promise<void>;
  onDelete: (id: string) => Promise<void>;
}

function FolderTreeRow({
  node,
  onAddChild,
  onDelete,
}: FolderTreeRowProps) {
  const { t } = useI18n();
  return (
    <>
      <div
        className={`folder-item ${node.depth > 0 ? "subfolder" : ""}`}
        style={{ paddingLeft: `${node.depth * 20}px` }}
      >
        <span>{node.name}</span>
        <div className="folder-actions">
          <button
            className="btn-icon add-subfolder-btn"
            onClick={() => onAddChild(node.id)}
            title={t("createSubfolder")}
          >
            <svg
              width="16"
              height="16"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
            >
              <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
              <line x1="12" y1="11" x2="12" y2="17" />
              <line x1="9" y1="14" x2="15" y2="14" />
            </svg>
          </button>
          <button
            className="btn-icon delete-btn"
            onClick={() => onDelete(node.id)}
            title={t("deleteFolderTitle")}
          >
            <svg
              width="16"
              height="16"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
            >
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>
      </div>
      {node.children.map((child) => (
        <FolderTreeRow
          key={child.id}
          node={child}
          onAddChild={onAddChild}
          onDelete={onDelete}
        />
      ))}
    </>
  );
}
