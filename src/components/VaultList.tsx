import { useI18n } from "../hooks/useI18n";
import type { VaultEntry, VaultFilter } from "../types";

interface VaultListProps {
  entries: VaultEntry[];
  filter: VaultFilter;
  selectedId: string | null;
  searchQuery: string;
  onSearch: (query: string) => void;
  onSelect: (entry: VaultEntry) => void;
  onNew: () => void;
}

export default function VaultList({
  entries,
  filter,
  selectedId,
  searchQuery,
  onSearch,
  onSelect,
  onNew,
}: VaultListProps) {
  const { t } = useI18n();

  const filtered = entries.filter((e) => {
    if (filter === "logins") return e.entryType === "login";
    if (filter === "apikeys") return e.entryType === "apiKey";
    if (filter === "favorites") return e.favorite;
    return true;
  });

  const getInitials = (name: string) =>
    name
      .split(" ")
      .map((w) => w[0])
      .join("")
      .toUpperCase()
      .slice(0, 2);

  const getTypeIcon = (type: string) => {
    if (type === "apiKey") return "API";
    return "PWD";
  };

  return (
    <div className="vault-list">
      <div className="vault-list-header">
        <div className="search-bar">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="11" cy="11" r="8" />
            <line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
          <input
            type="text"
            placeholder={t("search")}
            value={searchQuery}
            onChange={(e) => onSearch(e.target.value)}
          />
        </div>
        <button className="btn-new" onClick={onNew} title={t("addEntry")}>
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <line x1="12" y1="5" x2="12" y2="19" />
            <line x1="5" y1="12" x2="19" y2="12" />
          </svg>
        </button>
      </div>
      <div className="vault-items">
        {filtered.length === 0 && (
          <div className="empty-state">
            <p>{t("noEntries")}</p>
            <button className="btn-text" onClick={onNew}>{t("addFirstEntry")}</button>
          </div>
        )}
        {filtered.map((entry) => (
          <div
            key={entry.id}
            className={`vault-item ${selectedId === entry.id ? "active" : ""}`}
            onClick={() => onSelect(entry)}
          >
            <div className="vault-item-avatar">
              {getInitials(entry.name)}
            </div>
            <div className="vault-item-info">
              <div className="vault-item-name">
                {entry.favorite && <span className="star">★</span>}
                {entry.name}
              </div>
              <div className="vault-item-sub">
                {entry.username || entry.apiKey || entry.url || "—"}
              </div>
            </div>
            <span className={`vault-item-type type-${entry.entryType}`}>
              {getTypeIcon(entry.entryType)}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
