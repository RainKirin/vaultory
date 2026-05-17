import { describe, test, expect } from "vitest";
import { flattenTree } from "../useFolders";
import type { FolderNode } from "../../types";

describe("flattenTree", () => {
  test("深度优先展平嵌套树", () => {
    const tree: FolderNode[] = [
      {
        id: "a",
        name: "A",
        parentId: null,
        depth: 0,
        children: [
          { id: "b", name: "B", parentId: "a", depth: 1, children: [] },
        ],
      },
      { id: "c", name: "C", parentId: null, depth: 0, children: [] },
    ];
    const result = flattenTree(tree);
    expect(result.map((n) => n.id)).toEqual(["a", "b", "c"]);
  });

  test("空数组返回空", () => {
    expect(flattenTree([])).toEqual([]);
  });
});
