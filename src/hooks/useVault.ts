import { useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { VaultEntry, Folder, AppSettings } from "../types";

export function useVault() {
  const [entries, setEntries] = useState<VaultEntry[]>([]);
  const [folders, setFolders] = useState<Folder[]>([]);
  const [locked, setLocked] = useState(true);
  const [initialized, setInitialized] = useState<boolean | null>(null);
  const [settings, setSettingsState] = useState<AppSettings>({
    autoLockMinutes: 5,
    darkMode: false,
    language: "zh",
  });
  const [lastError, setLastError] = useState<string | null>(null);

  const checkInitialized = useCallback(async () => {
    const result = await invoke<boolean>("is_initialized");
    setInitialized(result);
    return result;
  }, []);

  const setupPassword = useCallback(async (password: string) => {
    await invoke("setup_master_password", { password });
    setLocked(false);
    setInitialized(true);
    await loadEntries();
    await loadFolders();
  }, []);

  const unlock = useCallback(async (password: string) => {
    const result = await invoke<boolean>("unlock", { password });
    if (result) {
      setLocked(false);
      await loadEntries();
      await loadFolders();
      await loadSettings();
    }
    return result;
  }, []);

  const lock = useCallback(async () => {
    await invoke("lock");
    setLocked(true);
    setEntries([]);
  }, []);

  const changeMasterPassword = useCallback(async (currentPassword: string, newPassword: string) => {
    try {
      await invoke("change_master_password", { currentPassword, newPassword });
      setLastError(null);
    } catch (e) {
      setLastError(`修改主密码失败: ${String(e)}`);
      throw e;
    }
  }, []);

  const resetVault = useCallback(async () => {
    try {
      await invoke("reset_vault");
      setLocked(true);
      setInitialized(false);
      setEntries([]);
      setFolders([]);
      setSettingsState({
        autoLockMinutes: 5,
        darkMode: false,
        language: "zh",
      });
      setLastError(null);
    } catch (e) {
      setLastError(`重置保险库失败: ${String(e)}`);
      throw e;
    }
  }, []);

  const loadEntries = useCallback(async () => {
    try {
      const result = await invoke<VaultEntry[]>("get_all_entries");
      setEntries(result);
      setLastError(null);
    } catch (e) {
      setLastError(`加载条目失败: ${String(e)}`);
    }
  }, []);

  const saveEntry = useCallback(async (entry: VaultEntry) => {
    try {
      await invoke("save_entry", { entry });
      setLastError(null);
    } catch (e) {
      setLastError(`保存失败: ${String(e)}`);
      throw e;
    }
    await loadEntries();
  }, [loadEntries]);

  const deleteEntry = useCallback(async (id: string) => {
    await invoke("delete_entry", { id });
    await loadEntries();
  }, [loadEntries]);

  const searchEntries = useCallback(async (query: string) => {
    if (!query.trim()) {
      await loadEntries();
      return;
    }
    const result = await invoke<VaultEntry[]>("search_entries", { query });
    setEntries(result);
  }, [loadEntries]);

  const loadFolders = useCallback(async () => {
    try {
      const result = await invoke<Folder[]>("get_folders");
      setFolders(result);
      setLastError(null);
    } catch (e) {
      setLastError(`加载文件夹失败: ${String(e)}`);
      throw e;
    }
  }, []);

  const saveFolder = useCallback(async (folder: Folder) => {
    try {
      await invoke("save_folder", { folder });
      setLastError(null);
      await loadFolders();
    } catch (e) {
      setLastError(`保存文件夹失败: ${String(e)}`);
      throw e;
    }
  }, [loadFolders]);

  const deleteFolder = useCallback(async (id: string, strategy: string = "merge_up") => {
    try {
      await invoke("delete_folder", { id, strategy });
      setLastError(null);
      await Promise.all([loadFolders(), loadEntries()]);
    } catch (e) {
      setLastError(`删除文件夹失败: ${String(e)}`);
      throw e;
    }
  }, [loadEntries, loadFolders]);

  const loadSettings = useCallback(async () => {
    const result = await invoke<AppSettings>("get_settings");
    setSettingsState(result);
  }, []);

  const saveSettings = useCallback(async (s: AppSettings) => {
    await invoke("save_settings", { settings: s });
    setSettingsState(s);
  }, []);

  const consumeMigrationNotice = useCallback(async () => {
    try {
      return await invoke<string | null>("consume_migration_notice");
    } catch {
      return null;
    }
  }, []);

  return {
    entries,
    folders,
    locked,
    initialized,
    settings,
    lastError,
    checkInitialized,
    setupPassword,
    unlock,
    lock,
    changeMasterPassword,
    resetVault,
    loadEntries,
    saveEntry,
    deleteEntry,
    searchEntries,
    saveFolder,
    deleteFolder,
    saveSettings,
    consumeMigrationNotice,
  };
}
