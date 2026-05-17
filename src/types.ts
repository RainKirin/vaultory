export type EntryType = "login" | "apiKey";

export interface VaultEntry {
  id: string;
  entryType: EntryType;
  name: string;
  username?: string;
  password?: string;
  url?: string;
  apiKey?: string;
  notes?: string;
  folder?: string;
  favorite: boolean;
  createdAt: number;
  updatedAt: number;
}

export interface Folder {
  id: string;
  name: string;
  parentId: string | null;
  sortOrder?: number;
}

export interface FolderNode extends Folder {
  children: FolderNode[];
  depth: number;
}

export type Language = "en" | "zh";

export interface AppSettings {
  autoLockMinutes: number;
  darkMode: boolean;
  language: Language;
}

export interface PasswordGeneratorConfig {
  length: number;
  uppercase: boolean;
  lowercase: boolean;
  numbers: boolean;
  symbols: boolean;
  excludeAmbiguous: boolean;
}

export interface GeneratedPassword {
  password: string;
  strength: string;
}

export interface UsernameConfig {
  format: string;
}

export type View = "vault" | "generator" | "settings";
export type VaultFilter = "all" | "logins" | "apikeys" | "favorites";
