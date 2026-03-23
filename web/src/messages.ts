/**
 * Message Helpers
 *
 * Pure functions for working with agent messages.
 * No browser or SDK dependencies.
 */

import type { AgentMessage } from "@mariozechner/pi-agent-core";

/** Extract a short title from the first user message. */
export function titleFromMessages(messages: AgentMessage[]): string {
  const first = messages.find(
    (m) => m.role === "user" || m.role === "user-with-attachments",
  );
  if (!first || (first.role !== "user" && first.role !== "user-with-attachments"))
    return "";

  const content = first.content;
  const text =
    typeof content === "string"
      ? content
      : (content as any[])
          .filter((c: any) => c.type === "text")
          .map((c: any) => c.text || "")
          .join(" ");

  const trimmed = text.trim();
  if (!trimmed) return "";
  return trimmed.length <= 60 ? trimmed : `${trimmed.substring(0, 57)}...`;
}

/** Check whether messages contain at least one user and one assistant message. */
export function hasConversation(messages: AgentMessage[]): boolean {
  return (
    messages.some((m: any) => m.role === "user" || m.role === "user-with-attachments") &&
    messages.some((m: any) => m.role === "assistant")
  );
}
