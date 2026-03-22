/**
 * Filesystem Tool Plugin
 *
 * Gives the agent eyes into the user's codebase via the
 * File System Access API. Two tools, no more:
 *
 * - list_directory: see what's in a directory
 * - read_file: read a file's contents
 *
 * The agent explores incrementally — list a directory, pick
 * interesting files, read them, reason about structure.
 * Intelligence lives in the model, not in the tooling.
 */

import { Type } from "@sinclair/typebox";
import type { AgentTool, AgentToolResult } from "@mariozechner/pi-agent-core";
import type { ToolPlugin, ToolContext } from "../tools.js";

// --- Ignore patterns ---

const IGNORED_DIRS = new Set([
  ".git",
  "node_modules",
  "target",
  "dist",
  "build",
  ".next",
  "__pycache__",
  ".venv",
  "venv",
  ".cache",
  ".turbo",
  "coverage",
]);

const IGNORED_FILES = new Set([
  ".DS_Store",
  "Thumbs.db",
  "package-lock.json",
]);

function shouldIgnore(name: string, isDir: boolean): boolean {
  return isDir ? IGNORED_DIRS.has(name) : IGNORED_FILES.has(name);
}

// --- Path resolution ---

/**
 * Resolve a path string (e.g., "src/agent/tools.ts") to a
 * FileSystemHandle by walking the directory tree segment by segment.
 */
async function resolvePath(
  root: FileSystemDirectoryHandle,
  path: string,
): Promise<FileSystemDirectoryHandle | FileSystemFileHandle> {
  const segments = path
    .split("/")
    .filter((s) => s.length > 0 && s !== ".");

  if (segments.length === 0) return root;

  let current: FileSystemDirectoryHandle = root;
  for (let i = 0; i < segments.length - 1; i++) {
    current = await current.getDirectoryHandle(segments[i]);
  }

  const last = segments[segments.length - 1];

  // Try directory first, then file
  try {
    return await current.getDirectoryHandle(last);
  } catch {
    return await current.getFileHandle(last);
  }
}

async function resolveDirectory(
  root: FileSystemDirectoryHandle,
  path?: string,
): Promise<FileSystemDirectoryHandle> {
  if (!path || path === "" || path === "." || path === "/") return root;
  const handle = await resolvePath(root, path);
  if (handle.kind !== "directory") {
    throw new Error(`"${path}" is a file, not a directory`);
  }
  return handle as FileSystemDirectoryHandle;
}

async function resolveFile(
  root: FileSystemDirectoryHandle,
  path: string,
): Promise<FileSystemFileHandle> {
  const handle = await resolvePath(root, path);
  if (handle.kind !== "file") {
    throw new Error(`"${path}" is a directory, not a file`);
  }
  return handle as FileSystemFileHandle;
}

// --- Tool: list_directory ---

const ListDirectorySchema = Type.Object({
  path: Type.Optional(
    Type.String({ description: 'Directory path relative to project root. Omit or use "" for root.' }),
  ),
});

type ListDirectoryParams = { path?: string };

function textResult(text: string): AgentToolResult<void> {
  return { content: [{ type: "text", text }], details: undefined as void };
}

function listDirectoryTool(root: FileSystemDirectoryHandle): AgentTool<any> {
  return {
    name: "list_directory",
    description:
      "List files and subdirectories at a path in the project. " +
      "Returns entry names with [dir] or [file] markers. " +
      "Use this to explore the project structure incrementally.",
    label: "List Directory",
    parameters: ListDirectorySchema,
    async execute(_toolCallId, params: ListDirectoryParams): Promise<AgentToolResult<void>> {
      try {
        const dir = await resolveDirectory(root, params.path);
        const entries: string[] = [];

        // @ts-expect-error — entries() is available in Chrome/Edge but not in all TS lib defs
        for await (const [name, handle] of dir.entries() as AsyncIterable<[string, FileSystemHandle]>) {
          const isDir = handle.kind === "directory";
          if (shouldIgnore(name, isDir)) continue;
          entries.push(isDir ? `[dir]  ${name}` : `[file] ${name}`);
        }

        entries.sort((a, b) => {
          // Directories first, then alphabetical
          const aDir = a.startsWith("[dir]");
          const bDir = b.startsWith("[dir]");
          if (aDir !== bDir) return aDir ? -1 : 1;
          return a.localeCompare(b);
        });

        if (entries.length === 0) {
          return textResult("(empty directory)");
        }

        const header = params.path ? params.path : root.name;
        return textResult(`${header}/\n${entries.join("\n")}`);
      } catch (e) {
        return textResult(`Error: ${(e as Error).message}`);
      }
    },
  };
}

// --- Tool: read_file ---

const ReadFileSchema = Type.Object({
  path: Type.String({ description: "File path relative to project root (e.g., \"src/main.ts\")." }),
});

type ReadFileParams = { path: string };

const MAX_FILE_SIZE = 256 * 1024; // 256 KB — large enough for any reasonable source file

function readFileTool(root: FileSystemDirectoryHandle): AgentTool<any> {
  return {
    name: "read_file",
    description:
      "Read the contents of a file in the project. " +
      "Returns the file text with line numbers. " +
      "Use this to understand implementation details after exploring with list_directory.",
    label: "Read File",
    parameters: ReadFileSchema,
    async execute(_toolCallId, params: ReadFileParams): Promise<AgentToolResult<void>> {
      try {
        const handle = await resolveFile(root, params.path);
        const file = await handle.getFile();

        if (file.size > MAX_FILE_SIZE) {
          return textResult(
            `File is ${(file.size / 1024).toFixed(0)} KB — too large to read in full. ` +
            `This is likely a generated or binary file.`,
          );
        }

        const text = await file.text();
        const lines = text.split("\n");
        const numbered = lines
          .map((line, i) => `${String(i + 1).padStart(4)}  ${line}`)
          .join("\n");

        return textResult(`${params.path} (${lines.length} lines)\n${numbered}`);
      } catch (e) {
        return textResult(`Error: ${(e as Error).message}`);
      }
    },
  };
}

// --- Plugin ---

export const filesystemPlugin: ToolPlugin = {
  id: "filesystem",
  label: "File System",

  available(ctx: ToolContext): boolean {
    return ctx.projectHandle !== undefined;
  },

  createTools(ctx: ToolContext): AgentTool[] {
    if (!ctx.projectHandle) return [];
    return [
      listDirectoryTool(ctx.projectHandle),
      readFileTool(ctx.projectHandle),
    ];
  },
};
