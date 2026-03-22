/**
 * NodeDirectoryHandle — test adapter
 *
 * Implements enough of the File System Access API surface for
 * the filesystem plugin to work against the real filesystem
 * in Node/vitest. This lets us test tool execution against
 * Canopy's own source tree without a browser.
 */

import * as fs from "node:fs";
import * as path from "node:path";

export class NodeFileHandle {
  readonly kind = "file" as const;
  readonly name: string;
  private filepath: string;

  constructor(filepath: string) {
    this.filepath = filepath;
    this.name = path.basename(filepath);
  }

  async getFile(): Promise<File> {
    const buffer = fs.readFileSync(this.filepath);
    const text = buffer.toString("utf-8");
    // Minimal File shim — enough for the plugin's .text() and .size
    return {
      size: buffer.length,
      text: async () => text,
      name: this.name,
    } as unknown as File;
  }
}

export class NodeDirectoryHandle {
  readonly kind = "directory" as const;
  readonly name: string;
  private dirpath: string;

  constructor(dirpath: string) {
    this.dirpath = dirpath;
    this.name = path.basename(dirpath);
  }

  async getDirectoryHandle(name: string): Promise<NodeDirectoryHandle> {
    const child = path.join(this.dirpath, name);
    if (!fs.existsSync(child) || !fs.statSync(child).isDirectory()) {
      throw new DOMException(`Directory "${name}" not found`, "NotFoundError");
    }
    return new NodeDirectoryHandle(child);
  }

  async getFileHandle(name: string): Promise<NodeFileHandle> {
    const child = path.join(this.dirpath, name);
    if (!fs.existsSync(child) || !fs.statSync(child).isFile()) {
      throw new DOMException(`File "${name}" not found`, "NotFoundError");
    }
    return new NodeFileHandle(child);
  }

  async *entries(): AsyncIterableIterator<[string, NodeDirectoryHandle | NodeFileHandle]> {
    const entries = fs.readdirSync(this.dirpath, { withFileTypes: true });
    for (const entry of entries) {
      const fullPath = path.join(this.dirpath, entry.name);
      if (entry.isDirectory()) {
        yield [entry.name, new NodeDirectoryHandle(fullPath)];
      } else if (entry.isFile()) {
        yield [entry.name, new NodeFileHandle(fullPath)];
      }
    }
  }
}
