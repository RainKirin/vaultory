import { useState, useEffect, useCallback, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Folder, FolderNode } from '../types';

export function useFolders(externalFolders?: Folder[]) {
  const [flat, setFlat] = useState<Folder[]>(externalFolders ?? []);
  const [expanded, setExpanded] = useState<Set<string>>(() => {
    try {
      const stored = localStorage.getItem('folder-expanded');
      return stored ? new Set(JSON.parse(stored)) : new Set();
    } catch {
      return new Set();
    }
  });

  const loadFolders = useCallback(async () => {
    try {
      const folders = await invoke<Folder[]>('get_folders');
      setFlat(folders);
    } catch (e) {
      console.error('Failed to load folders:', e);
    }
  }, []);

  useEffect(() => {
    if (externalFolders) {
      setFlat(externalFolders);
      return;
    }

    loadFolders();
  }, [externalFolders, loadFolders]);

  useEffect(() => {
    localStorage.setItem('folder-expanded', JSON.stringify([...expanded]));
  }, [expanded]);

  const tree = useMemo(() => buildTree(flat), [flat]);

  const toggleExpanded = useCallback((id: string) => {
    setExpanded(prev => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  const create = useCallback(async (name: string, parentId: string | null = null) => {
    const folder: Folder = {
      id: crypto.randomUUID(),
      name,
      parentId,
      sortOrder: Date.now(),
    };
    await invoke('save_folder', { folder });
    await loadFolders();
  }, [loadFolders]);

  const rename = useCallback(async (id: string, name: string) => {
    const folder = flat.find(f => f.id === id);
    if (!folder) return;
    await invoke('save_folder', { folder: { ...folder, name } });
    await loadFolders();
  }, [flat]);

  const move = useCallback(async (id: string, newParentId: string | null) => {
    const folder = flat.find(f => f.id === id);
    if (!folder) return;
    await invoke('save_folder', { folder: { ...folder, parentId: newParentId } });
    await loadFolders();
  }, [flat]);

  const remove = useCallback(async (id: string, strategy: 'merge_up' | 'cascade' = 'merge_up') => {
    await invoke('delete_folder', { id, strategy });
    await loadFolders();
  }, [loadFolders]);

  return { flat, tree, expanded, toggleExpanded, reload: loadFolders, create, rename, move, remove };
}

function buildTree(flat: Folder[]): FolderNode[] {
  const map = new Map<string, FolderNode>();
  flat.forEach(f => map.set(f.id, { ...f, children: [], depth: 0 }));
  
  const roots: FolderNode[] = [];
  flat.forEach(f => {
    const node = map.get(f.id)!;
    if (f.parentId && map.has(f.parentId)) {
      const parent = map.get(f.parentId)!;
      node.depth = parent.depth + 1;
      parent.children.push(node);
    } else {
      roots.push(node);
    }
  });

  const sortFn = (a: FolderNode, b: FolderNode) =>
    (a.sortOrder ?? 0) - (b.sortOrder ?? 0) || a.name.localeCompare(b.name);
  
  const walk = (nodes: FolderNode[]) => {
    nodes.sort(sortFn);
    nodes.forEach(n => walk(n.children));
  };
  walk(roots);

  return roots;
}

export function flattenTree(nodes: FolderNode[]): FolderNode[] {
  const result: FolderNode[] = [];
  const walk = (ns: FolderNode[]) => {
    for (const n of ns) {
      result.push(n);
      walk(n.children);
    }
  };
  walk(nodes);
  return result;
}
