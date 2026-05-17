import { useI18n } from "../hooks/useI18n";
import { useFolders } from "../hooks/useFolders";
import type { View, VaultFilter, Folder, FolderNode } from "../types";

interface SidebarProps {
  view: View;
  filter: VaultFilter;
  folders: Folder[];
  selectedFolder: string | null;
  onViewChange: (view: View) => void;
  onFilterChange: (filter: VaultFilter) => void;
  onFolderSelect: (folder: string | null) => void;
  onLock: () => void;
}

export default function Sidebar({
  view,
  filter,
  folders,
  selectedFolder,
  onViewChange,
  onFilterChange,
  onFolderSelect,
  onLock,
}: SidebarProps) {
  const { t } = useI18n();
  const { tree, expanded, toggleExpanded } = useFolders(folders);

  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <div className="sidebar-logo">
          <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <path d="M12 2L2 7l10 5 10-5-10-5z" />
            <path d="M2 17l10 5 10-5" />
            <path d="M2 12l10 5 10-5" />
          </svg>
          <span>{t("vault")}</span>
        </div>
      </div>

      <nav className="sidebar-nav">
        <button
          className={`nav-item ${view === "vault" && filter === "all" ? "active" : ""}`}
          onClick={() => { onViewChange("vault"); onFilterChange("all"); onFolderSelect(null); }}
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <rect x="3" y="3" width="7" height="7" rx="1" />
            <rect x="14" y="3" width="7" height="7" rx="1" />
            <rect x="3" y="14" width="7" height="7" rx="1" />
            <rect x="14" y="14" width="7" height="7" rx="1" />
          </svg>
          {t("allItems")}
        </button>
        <button
          className={`nav-item ${view === "vault" && filter === "logins" ? "active" : ""}`}
          onClick={() => { onViewChange("vault"); onFilterChange("logins"); onFolderSelect(null); }}
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <rect x="3" y="11" width="18" height="11" rx="2" />
            <path d="M7 11V7a5 5 0 0 1 10 0v4" />
          </svg>
          {t("logins")}
        </button>
        <button
          className={`nav-item ${view === "vault" && filter === "apikeys" ? "active" : ""}`}
          onClick={() => { onViewChange("vault"); onFilterChange("apikeys"); onFolderSelect(null); }}
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <path d="M21 2l-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 0 1 7.777-7.777zm0 0L15.5 7.5m0 0l3 3L22 7l-3-3m-3.5 3.5L19 4" />
          </svg>
          {t("apiKeys")}
        </button>
        <button
          className={`nav-item ${view === "vault" && filter === "favorites" ? "active" : ""}`}
          onClick={() => { onViewChange("vault"); onFilterChange("favorites"); onFolderSelect(null); }}
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
          </svg>
          {t("favorites")}
        </button>

        {tree.length > 0 && (
          <div className="nav-divider">
            <span>{t("folders")}</span>
          </div>
        )}
        {tree.map((node) => (
          <FolderTreeItem
            key={node.id}
            node={node}
            selectedFolder={selectedFolder}
            expanded={expanded}
            onToggle={toggleExpanded}
            onSelect={onFolderSelect}
            onViewChange={onViewChange}
            onFilterChange={onFilterChange}
          />
        ))}

        <div className="nav-divider">
          <span>{t("tools")}</span>
        </div>
        <button
          className={`nav-item ${view === "generator" ? "active" : ""}`}
          onClick={() => onViewChange("generator")}
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <path d="M12 2v20M2 12h20M4.93 4.93l14.14 14.14M19.07 4.93L4.93 19.07" />
          </svg>
          {t("generator")}
        </button>
        <button
          className={`nav-item ${view === "settings" ? "active" : ""}`}
          onClick={() => onViewChange("settings")}
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <circle cx="12" cy="12" r="3" />
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" />
          </svg>
          {t("settings")}
        </button>
      </nav>

      <div className="sidebar-footer">
        <button className="nav-item lock-btn" onClick={onLock}>
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <rect x="3" y="11" width="18" height="11" rx="2" />
            <path d="M7 11V7a5 5 0 0 1 10 0v4" />
          </svg>
          {t("lockVault")}
        </button>
      </div>
    </aside>
  );
}

interface FolderTreeItemProps {
  node: FolderNode;
  selectedFolder: string | null;
  expanded: Set<string>;
  onToggle: (id: string) => void;
  onSelect: (folder: string | null) => void;
  onViewChange: (view: View) => void;
  onFilterChange: (filter: VaultFilter) => void;
}

function FolderTreeItem({
  node,
  selectedFolder,
  expanded,
  onToggle,
  onSelect,
  onViewChange,
  onFilterChange,
}: FolderTreeItemProps) {
  const isExpanded = expanded.has(node.id);
  const hasChildren = node.children.length > 0;

  return (
    <>
      <button
        className={`nav-item folder-item ${selectedFolder === node.id ? "active" : ""}`}
        style={{ paddingLeft: `${12 + node.depth * 16}px` }}
        onClick={() => { onViewChange("vault"); onFilterChange("all"); onSelect(node.id); }}
      >
        {hasChildren && (
          <span
            className="folder-toggle"
            onClick={(e) => { e.stopPropagation(); onToggle(node.id); }}
          >
            {isExpanded ? "▼" : "▶"}
          </span>
        )}
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
          <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
        </svg>
        <span className="folder-name">{node.name}</span>
      </button>
      {isExpanded && hasChildren && (
        <div className="folder-children">
          {node.children.map((child: FolderNode) => (
            <FolderTreeItem
              key={child.id}
              node={child}
              selectedFolder={selectedFolder}
              expanded={expanded}
              onToggle={onToggle}
              onSelect={onSelect}
              onViewChange={onViewChange}
              onFilterChange={onFilterChange}
            />
          ))}
        </div>
      )}
    </>
  );
}
