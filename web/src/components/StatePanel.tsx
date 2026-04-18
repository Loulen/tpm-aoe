import { useCallback, useEffect, useRef, useState } from "react";
import type { SessionResponse } from "../lib/types";

const POLL_INTERVAL_MS = 2500;

interface Props {
  session: SessionResponse | null;
  onClose: () => void;
}

/** Fetch STATE.md content for a session via its project path. */
async function fetchStateMd(sessionId: string): Promise<string | null> {
  try {
    const res = await fetch(`/api/sessions/${sessionId}/state`);
    if (!res.ok) return null;
    const data = await res.json();
    return data.content ?? null;
  } catch {
    return null;
  }
}

/** Determine status color class from cell text. */
function statusClass(text: string): string {
  const lower = text.trim().toLowerCase();
  if (
    lower.includes("done") ||
    lower.includes("completed") ||
    lower.includes("\u2705")
  )
    return "text-status-running";
  if (
    lower.includes("implementing") ||
    lower.includes("in_progress") ||
    lower.includes("in-progress") ||
    lower.includes("running") ||
    lower.includes("\ud83d\udfe1")
  )
    return "text-status-waiting";
  if (lower.includes("reviewing") || lower.includes("pending"))
    return "text-brand-500";
  if (
    lower.includes("blocked") ||
    lower.includes("failed") ||
    lower.includes("\u274c")
  )
    return "text-status-error";
  if (
    lower.includes("not-started") ||
    lower.includes("not_started") ||
    lower.includes("skipped")
  )
    return "text-text-dim";
  return "text-text-secondary";
}

/** Check if a table line is a separator row. */
function isTableSeparator(line: string): boolean {
  const inner = line.replace(/^\||\|$/g, "").trim();
  return inner.length > 0 && /^[-|: ]+$/.test(inner);
}

/** Parse a table row into trimmed cells. */
function parseTableRow(line: string): string[] {
  let s = line.trim();
  if (s.startsWith("|")) s = s.slice(1);
  if (s.endsWith("|")) s = s.slice(0, -1);
  return s.split("|").map((c) => c.trim());
}

/** Render a markdown table block as an HTML table. */
function MarkdownTable({ lines }: { lines: string[] }) {
  const dataLines = lines.filter((l) => !isTableSeparator(l));
  if (dataLines.length === 0) return null;

  const header = parseTableRow(dataLines[0]!);
  const rows = dataLines.slice(1).map(parseTableRow);

  return (
    <div className="overflow-x-auto my-1">
      <table className="text-xs border-collapse w-full">
        <thead>
          <tr>
            {header.map((cell, i) => (
              <th
                key={i}
                className="text-left px-2 py-0.5 text-text-primary font-semibold border-b border-surface-700/30"
              >
                {cell}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, ri) => (
            <tr
              key={ri}
              className="hover:bg-surface-800/30 transition-colors"
            >
              {row.map((cell, ci) => (
                <td
                  key={ci}
                  className={`px-2 py-0.5 border-b border-surface-700/10 ${statusClass(cell)}`}
                >
                  {cell}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** Render STATE.md content as styled HTML. */
function StateContent({ content }: { content: string }) {
  const lines = content.split("\n");
  const elements: React.ReactNode[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i]!;
    const trimmed = line.trim();

    // Collect table blocks
    if (trimmed.startsWith("|") && (trimmed.match(/\|/g) ?? []).length >= 2) {
      const tableLines: string[] = [];
      while (i < lines.length) {
        const cur = lines[i]!.trim();
        if (!cur.startsWith("|") || (cur.match(/\|/g) ?? []).length < 2) break;
        tableLines.push(cur);
        i++;
      }
      elements.push(<MarkdownTable key={elements.length} lines={tableLines} />);
      continue;
    }

    // Headers
    if (trimmed.startsWith("### ")) {
      elements.push(
        <h4
          key={elements.length}
          className="text-xs font-semibold text-text-secondary mt-2 mb-0.5 italic"
        >
          {trimmed.slice(4)}
        </h4>,
      );
      i++;
      continue;
    }
    if (trimmed.startsWith("## ")) {
      elements.push(
        <h3
          key={elements.length}
          className="text-sm font-bold text-text-primary mt-3 mb-1"
        >
          {trimmed.slice(3)}
        </h3>,
      );
      i++;
      continue;
    }
    if (trimmed.startsWith("# ")) {
      elements.push(
        <h2
          key={elements.length}
          className="text-sm font-bold text-text-primary mt-2 mb-1"
        >
          {trimmed.slice(2)}
        </h2>,
      );
      i++;
      continue;
    }

    // Empty line
    if (trimmed === "") {
      elements.push(<div key={elements.length} className="h-1" />);
      i++;
      continue;
    }

    // Bullet points
    if (trimmed.startsWith("- ") || trimmed.startsWith("* ")) {
      const bulletText = trimmed.slice(2);
      elements.push(
        <div
          key={elements.length}
          className={`text-xs pl-3 ${statusClass(bulletText)}`}
        >
          &bull; {bulletText}
        </div>,
      );
      i++;
      continue;
    }

    // Regular text
    elements.push(
      <p
        key={elements.length}
        className={`text-xs ${statusClass(trimmed)}`}
      >
        {trimmed}
      </p>,
    );
    i++;
  }

  return <>{elements}</>;
}

export function StatePanel({ session, onClose }: Props) {
  const [content, setContent] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const poll = useCallback(async () => {
    if (!session) return;
    const md = await fetchStateMd(session.id);
    setContent(md);
    setLoading(false);
  }, [session]);

  useEffect(() => {
    setLoading(true);
    setContent(null);
    poll();
    timerRef.current = setInterval(poll, POLL_INTERVAL_MS);
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [poll]);

  // No content = no panel (hide gracefully)
  if (!loading && content === null) return null;

  return (
    <div className="flex-1 flex flex-col min-h-0 overflow-hidden border-l border-surface-700/20">
      {/* Header */}
      <div className="h-8 flex items-center px-3 border-b border-surface-700/20 shrink-0 bg-surface-900">
        <span className="text-xs font-semibold text-brand-500 flex-1">
          TPM State
        </span>
        <button
          onClick={onClose}
          className="w-6 h-6 flex items-center justify-center text-text-dim hover:text-text-secondary hover:bg-surface-800 cursor-pointer rounded transition-colors text-sm"
        >
          &times;
        </button>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto px-3 py-2">
        {loading ? (
          <span className="text-xs text-text-dim">Loading...</span>
        ) : content ? (
          <StateContent content={content} />
        ) : (
          <span className="text-xs text-text-dim">
            No STATE.md found for this session.
          </span>
        )}
      </div>
    </div>
  );
}
