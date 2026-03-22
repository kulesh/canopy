/**
 * Project Directory Management
 *
 * Manages the user's project directory via the File System Access API.
 * The directory handle is the single capability that unlocks file-based
 * tools — once the user grants access, the agent can explore their codebase.
 */

/** Prompt the user to select a project directory. */
export async function selectProjectDirectory(): Promise<FileSystemDirectoryHandle> {
  // @ts-expect-error — showDirectoryPicker is not yet in all TS lib definitions
  const handle: FileSystemDirectoryHandle = await window.showDirectoryPicker({
    mode: "read",
  });
  return handle;
}

/** Check whether the File System Access API is available in this browser. */
export function isFileSystemAccessSupported(): boolean {
  return "showDirectoryPicker" in window;
}
