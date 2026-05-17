import { useState, useEffect, useCallback, useRef, useMemo } from "react";
import { useVault } from "./hooks/useVault";
import { I18nContext } from "./hooks/useI18n";
import { translations } from "./i18n/translations";
import type { Lang } from "./i18n/translations";
import LockScreen from "./components/LockScreen";
import Sidebar from "./components/Sidebar";
import VaultList from "./components/VaultList";
import VaultDetail from "./components/VaultDetail";
import Generator from "./components/Generator";
import Settings from "./components/Settings";
import type { VaultEntry, View, VaultFilter } from "./types";

export default function App() {
  const vault = useVault();
  const [view, setView] = useState<View>("vault");
  const [filter, setFilter] = useState<VaultFilter>("all");
  const [selectedFolder, setSelectedFolder] = useState<string | null>(null);
  const [selectedEntry, setSelectedEntry] = useState<VaultEntry | null>(null);
  const [isCreating, setIsCreating] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const idleTimerRef = useRef<number | null>(null);
  const [lang, setLangState] = useState<Lang>("zh");

  const [error, setError] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);

  useEffect(() => {
    vault.checkInitialized().catch((e) => {
      console.error("Init error:", e);
      setError(String(e));
    });
  }, []);

  // Sync language from settings
  useEffect(() => {
    if (vault.settings.language) {
      setLangState(vault.settings.language);
    }
  }, [vault.settings.language]);

  // Auto-lock timer
  useEffect(() => {
    if (vault.locked) return;

    const resetTimer = () => {
      if (idleTimerRef.current) window.clearTimeout(idleTimerRef.current);
      idleTimerRef.current = window.setTimeout(() => {
        vault.lock();
      }, vault.settings.autoLockMinutes * 60 * 1000);
    };

    const events = ["mousedown", "keydown", "scroll", "mousemove"];
    events.forEach((e) => window.addEventListener(e, resetTimer));
    resetTimer();

    return () => {
      events.forEach((e) => window.removeEventListener(e, resetTimer));
      if (idleTimerRef.current) window.clearTimeout(idleTimerRef.current);
    };
  }, [vault.locked, vault.settings.autoLockMinutes]);

  // Dark mode
  useEffect(() => {
    document.body.classList.toggle("dark", vault.settings.darkMode);
  }, [vault.settings.darkMode]);

  // 同步 HTML lang 属性（辅助可访问性 / 字体渲染）
  useEffect(() => {
    document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";
  }, [lang]);

  const handleSearch = useCallback(
    async (q: string) => {
      setSearchQuery(q);
      await vault.searchEntries(q);
    },
    [vault.searchEntries]
  );

  const handleNewEntry = () => {
    setSelectedEntry(null);
    setIsCreating(true);
  };

  const handleSelectEntry = (entry: VaultEntry) => {
    setSelectedEntry(entry);
    setIsCreating(false);
  };

  const handleSave = async (entry: VaultEntry) => {
    await vault.saveEntry(entry);
    setIsCreating(false);
    if (entry.id) {
      setSelectedEntry(entry);
    }
  };

  const handleCancel = () => {
    setIsCreating(false);
    setSelectedEntry(null);
  };

  const handleSetLang = useCallback(async (l: Lang) => {
    setLangState(l);
    if (!vault.locked) {
      try {
        await vault.saveSettings({ ...vault.settings, language: l });
      } catch {
        // 错误已由 useVault 记录到 lastError，此处静默
      }
    }
  }, [vault.locked, vault.settings, vault.saveSettings]);

  const t = useCallback(
    (key: string): string => {
      return translations[lang]?.[key as keyof typeof translations.en] ?? translations.en[key as keyof typeof translations.en] ?? key;
    },
    [lang]
  );

  const i18nValue = useMemo(() => ({ lang, t, setLang: handleSetLang }), [lang, t, handleSetLang]);

  // 显示迁移通知（旧版「永不锁定」自动迁移后的一次性提示）
  useEffect(() => {
    if (vault.locked) return;
    vault.consumeMigrationNotice().then((notice) => {
      if (notice) setToast(notice);
    }).catch(() => {});
  }, [vault.locked]);

  // Show toast error for vault errors
  useEffect(() => {
    if (vault.lastError) {
      console.error(vault.lastError);
      setToast(vault.lastError);
      const timer = setTimeout(() => setToast(null), 8000);
      return () => clearTimeout(timer);
    }
  }, [vault.lastError]);

  useEffect(() => {
    if (selectedFolder && !vault.folders.some((folder) => folder.id === selectedFolder)) {
      setSelectedFolder(null);
    }
  }, [selectedFolder, vault.folders]);

  useEffect(() => {
    if (selectedEntry && !vault.entries.some((entry) => entry.id === selectedEntry.id)) {
      setSelectedEntry(null);
      setIsCreating(false);
    }
  }, [selectedEntry, vault.entries]);

  const selectedFolderIds = useMemo(() => {
    if (!selectedFolder) {
      return null;
    }

    const ids = new Set<string>([selectedFolder]);
    const queue = [selectedFolder];

    while (queue.length > 0) {
      const currentId = queue.shift()!;
      for (const folder of vault.folders) {
        if (folder.parentId === currentId && !ids.has(folder.id)) {
          ids.add(folder.id);
          queue.push(folder.id);
        }
      }
    }

    return ids;
  }, [selectedFolder, vault.folders]);

  const displayedEntries = selectedFolderIds
    ? vault.entries.filter((entry) => entry.folder && selectedFolderIds.has(entry.folder))
    : vault.entries;

  const toastNode = toast ? (
    <div
      style={{
        position: "fixed",
        top: 16,
        right: 16,
        zIndex: 9999,
        padding: "12px 20px",
        borderRadius: 12,
        background: "#ff3b30",
        color: "white",
        fontSize: 14,
        fontFamily: "-apple-system, sans-serif",
        boxShadow: "0 4px 16px rgba(255,59,48,0.3)",
        maxWidth: 400,
        wordBreak: "break-word",
        cursor: "pointer",
      }}
      onClick={() => setToast(null)}
    >
      {toast}
    </div>
  ) : null;

  if (error) {
    return (
      <div style={{ padding: 40, fontFamily: "monospace" }}>
        <h2>出错了</h2>
        <pre style={{ color: "red", whiteSpace: "pre-wrap" }}>{error}</pre>
      </div>
    );
  }

  if (vault.initialized === null) return null;

  if (vault.locked) {
    return (
      <I18nContext.Provider value={i18nValue}>
        <>
          {toastNode}
          <LockScreen
            initialized={vault.initialized}
            onSetup={vault.setupPassword}
            onUnlock={vault.unlock}
            onResetVault={vault.resetVault}
          />
        </>
      </I18nContext.Provider>
    );
  }

  return (
    <I18nContext.Provider value={i18nValue}>
      <>
        {toastNode}
        <div className="app-layout">
          <Sidebar
            view={view}
            filter={filter}
            folders={vault.folders}
            selectedFolder={selectedFolder}
            onViewChange={setView}
            onFilterChange={setFilter}
            onFolderSelect={setSelectedFolder}
            onLock={vault.lock}
          />
          <main className="main-content">
            {view === "vault" && (
              <div className="vault-layout">
                <VaultList
                  entries={displayedEntries}
                  filter={filter}
                  selectedId={selectedEntry?.id || null}
                  searchQuery={searchQuery}
                  onSearch={handleSearch}
                  onSelect={handleSelectEntry}
                  onNew={handleNewEntry}
                />
                <VaultDetail
                  entry={selectedEntry}
                  isNew={isCreating}
                  onSave={handleSave}
                  onDelete={vault.deleteEntry}
                  onCancel={handleCancel}
                />
              </div>
            )}
            {view === "generator" && <Generator />}
            {view === "settings" && (
              <Settings
                settings={vault.settings}
                folders={vault.folders}
                onSaveSettings={vault.saveSettings}
                onSaveFolder={vault.saveFolder}
                onDeleteFolder={vault.deleteFolder}
                onChangeMasterPassword={vault.changeMasterPassword}
              />
            )}
          </main>
        </div>
      </>
    </I18nContext.Provider>
  );
}
