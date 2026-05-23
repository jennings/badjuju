import { existsSync, promises as fs } from "node:fs";
import * as path from "node:path";
import {
  commands,
  type Disposable,
  EventEmitter,
  type ExtensionContext,
  languages,
  Position,
  Range,
  Selection,
  type TextDocument,
  type TextDocumentContentProvider,
  type TextEditor,
  TextEditorRevealType,
  Uri,
  window,
  workspace,
} from "vscode";
import {
  type Executable,
  LanguageClient,
  type LanguageClientOptions,
  type ServerOptions,
} from "vscode-languageclient/node";

declare const __BADJUJU_COMMIT__: string;
declare const __BADJUJU_VERSION__: string;

let client: LanguageClient;

const READONLY_SCHEME = "badjuju-status";

class StatusContentProvider implements TextDocumentContentProvider {
  private readonly _onDidChange = new EventEmitter<Uri>();
  readonly onDidChange = this._onDidChange.event;

  async provideTextDocumentContent(uri: Uri): Promise<string> {
    try {
      return await fs.readFile(uri.fsPath, "utf-8");
    } catch {
      return "";
    }
  }

  refresh(uri: Uri): void {
    this._onDidChange.fire(uri);
  }
}

const statusProvider = new StatusContentProvider();

const LOG_SHORTCUT_LINE_RE = /^JJ:\s+([A-Za-z][\w ]*?):\s+(.+)$/;
const STATUS_FILE_LINE_RE = /^([MADCR])\s+(.+)$/;
// Matches a `jj log --stat` per-file line, e.g. "│  src/main.rs | 3 +++".
// Skips the summary line ("N files changed, ...") because it has no " | <N> <+/->".
const STAT_LINE_RE =
  /^[\s│○●◆~*╭╮╯╰─├┤┬┴┼]*\s(\S[\S ]*?)\s+\|\s+\d+\s+[+-]+\s*$/;
// A commit header line in jj log output: graph node char + spaces + change_id.
// Graph node chars: @ (current), ○ ● (other), ◆ (immutable), * (rare). NOT ~ (elided continuation).
const COMMIT_HEADER_RE = /^[@○●◆*]\s+([a-z]+)\b/;

/** Parse a status.jujutsu line and return the file path it refers to, or null. */
export function parseStatusFile(line: string): string | null {
  const m = line.match(STATUS_FILE_LINE_RE) ?? line.match(STAT_LINE_RE);
  if (!m) return null;
  const rest = m[m.length - 1].trim();
  // jj renders renames/copies as "old => new" — squash needs the destination path.
  const arrow = rest.lastIndexOf(" => ");
  return arrow >= 0 ? rest.slice(arrow + 4).trim() : rest;
}

/**
 * Return the revision that owns the file at the cursor.
 *
 * - STATUS-section lines (matched by STATUS_FILE_LINE_RE) belong to the working copy → "@".
 * - Walks up from `cursorLine` (inclusive) so a cursor parked on a commit header
 *   returns that commit, not the one above it.
 * - Hitting the STATUS section header without finding a commit means we were in
 *   the STATUS file list with no commit context → working copy.
 */
export function findRevisionForLine(
  lines: readonly string[],
  cursorLine: number,
): string {
  const current = lines[cursorLine] ?? "";
  if (STATUS_FILE_LINE_RE.test(current)) return "@";
  for (let i = cursorLine; i >= 0; i--) {
    const text = lines[i] ?? "";
    const h = text.match(COMMIT_HEADER_RE);
    if (h) return h[1];
    if (text.startsWith("STATUS:")) return "@";
  }
  return "@";
}

/**
 * Return the change_id of the commit at or above the cursor in a log.jujutsu buffer.
 *
 * Walks up from `cursorLine` (inclusive) looking for a commit header line. Returns
 * null if none is found (e.g. cursor is inside the REVSET header section).
 */
export function findLogRevision(
  lines: readonly string[],
  cursorLine: number,
): string | null {
  for (let i = cursorLine; i >= 0; i--) {
    const m = (lines[i] ?? "").match(COMMIT_HEADER_RE);
    if (m) return m[1];
  }
  return null;
}

function isStatusFile(uri: Uri): boolean {
  return uri.path.endsWith("/status.jujutsu");
}

function isLogFile(uri: Uri): boolean {
  return uri.path.endsWith("/log.jujutsu");
}

function isDiffFile(uri: Uri): boolean {
  return uri.path.endsWith("/diff.jujutsu");
}

function isDescribeFile(uri: Uri): boolean {
  return uri.path.endsWith("/describe.jujutsu");
}

// Generated buffers the server rewrites on every command must open through
// the readonly scheme. log.jujutsu is intentionally excluded — its REVSET
// header is editable and re-runs the query on save.
function isReadonlyOutput(uri: Uri): boolean {
  return isStatusFile(uri) || isDiffFile(uri);
}

function waitForDocumentChange(
  doc: TextDocument,
  timeoutMs: number,
): Promise<void> {
  return new Promise((resolve) => {
    const disposables: Disposable[] = [];
    const done = () => {
      for (const d of disposables) d.dispose();
      resolve();
    };
    disposables.push(
      workspace.onDidChangeTextDocument((e) => {
        if (e.document === doc) done();
      }),
    );
    const timer = setTimeout(done, timeoutMs);
    disposables.push({ dispose: () => clearTimeout(timer) });
  });
}

function updateLogShortcutContext(editor: TextEditor | undefined): void {
  let onShortcutLine = false;
  if (editor && isLogFile(editor.document.uri)) {
    const lineText = editor.document.lineAt(editor.selection.active.line).text;
    onShortcutLine = LOG_SHORTCUT_LINE_RE.test(lineText);
  }
  commands.executeCommand(
    "setContext",
    "badjuju.onLogShortcutLine",
    onShortcutLine,
  );
}

function toReadonlyUri(fileUri: Uri): Uri {
  return fileUri.with({ scheme: READONLY_SCHEME });
}

function toFileUri(readonlyUri: Uri): Uri {
  return readonlyUri.with({ scheme: "file" });
}

async function openServerResult(
  resultUri: string,
  opts: { aside?: boolean } = {},
): Promise<void> {
  const parsed = Uri.parse(resultUri);
  const showOpts = {
    preserveFocus: false,
    viewColumn: opts.aside ? -2 : undefined, // -2 = ViewColumn.Beside
  };
  if (parsed.scheme === "file" && isReadonlyOutput(parsed)) {
    const readonlyUri = toReadonlyUri(parsed);
    statusProvider.refresh(readonlyUri);
    const doc = await workspace.openTextDocument(readonlyUri);
    await languages.setTextDocumentLanguage(doc, "jujutsu");
    await window.showTextDocument(doc, showOpts);
    return;
  }
  const doc = await workspace.openTextDocument(parsed);
  await window.showTextDocument(doc, showOpts);
}

/**
 * Build a `[{cursor: {uri, line}}]` argument array from the active editor's
 * cursor when it sits in a status or log buffer. Returns `undefined` if no
 * such editor — callers should fall through to a default (empty arguments,
 * server defaults to `@`).
 */
function cursorArgsForActiveEditor():
  | [{ cursor: { uri: string; line: number } }]
  | undefined {
  const editor = window.activeTextEditor;
  if (!editor) return undefined;
  const uri = editor.document.uri;
  if (!isStatusFile(uri) && !isLogFile(uri)) return undefined;
  return [
    {
      cursor: {
        uri: uri.toString(),
        line: editor.selection.active.line,
      },
    },
  ];
}

async function runFileScopedStatusCommand(
  serverCommand: string,
): Promise<void> {
  const editor = window.activeTextEditor;
  if (!editor) return;
  if (!isStatusFile(editor.document.uri)) return;
  const reloaded = waitForDocumentChange(editor.document, 1500);
  try {
    const result = await client.sendRequest("workspace/executeCommand", {
      command: serverCommand,
      arguments: [
        {
          cursor: {
            uri: editor.document.uri.toString(),
            line: editor.selection.active.line,
          },
        },
      ],
    });
    await openServerResult(result as string);
    await reloaded;
  } catch (e) {
    window.showInformationMessage(`${serverCommand}: ${(e as Error).message}`);
  }
}

function resolveServerCommand(context: ExtensionContext): string {
  if (process.env.SERVER_PATH) return process.env.SERVER_PATH;
  const binaryName = process.platform === "win32" ? "badjuju.exe" : "badjuju";
  const bundled = path.join(context.extensionPath, "out", "bin", binaryName);
  if (existsSync(bundled)) return bundled;
  return "badjuju";
}

export async function activate(context: ExtensionContext) {
  const traceOutputChannel = window.createOutputChannel(
    "Bad Juju - Jujutsu VCS",
  );
  const command = resolveServerCommand(context);
  const run: Executable = {
    command,
    args: ["lsp"],
    options: {
      env: {
        ...process.env,
        RUST_LOG: "info",
      },
    },
  };
  const serverOptions: ServerOptions = { run, debug: run };

  const config = workspace.getConfiguration("badjuju");
  const binaryPath: string | undefined = config.get("binaryPath");
  const keymapProfile: string = config.get("keymapProfile") ?? "magit";
  const initializationOptions: Record<string, unknown> = { keymapProfile };
  if (binaryPath) initializationOptions.binaryPath = binaryPath;

  // Set context keys so package.json keybindings can be gated by profile.
  // badjuju.keymapsActive is false only for 'none' (convenience for any when-clause).
  // badjuju.keymapProfile holds the raw string so profile-specific bindings can match.
  commands.executeCommand(
    "setContext",
    "badjuju.keymapsActive",
    keymapProfile !== "none",
  );
  commands.executeCommand("setContext", "badjuju.keymapProfile", keymapProfile);

  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: "file", language: "jujutsu" },
      { scheme: READONLY_SCHEME, language: "jujutsu" },
    ],
    synchronize: {
      fileEvents: workspace.createFileSystemWatcher("**/.jj/**"),
    },
    traceOutputChannel,
    initializationOptions,
  };

  // Watch badjuju output files so server-side rewrites (e.g. on describe save
  // regenerating status.jujutsu / log.jujutsu) propagate to open editors.
  // Regular file:// buffers (log.jujutsu) reload automatically; the readonly
  // scheme used for status/diff requires firing the content provider.
  const outputWatcher = workspace.createFileSystemWatcher(
    "**/.jj/badjuju/*.jujutsu",
  );
  outputWatcher.onDidChange((uri) => {
    if (isReadonlyOutput(uri)) {
      statusProvider.refresh(toReadonlyUri(uri));
    }
  });
  outputWatcher.onDidCreate((uri) => {
    if (isReadonlyOutput(uri)) {
      statusProvider.refresh(toReadonlyUri(uri));
    }
  });

  context.subscriptions.push(
    outputWatcher,
    workspace.registerTextDocumentContentProvider(
      READONLY_SCHEME,
      statusProvider,
    ),
    commands.registerCommand("badjuju.status.open", async () => {
      const result = await client.sendRequest("workspace/executeCommand", {
        command: "badjuju.status",
        arguments: [],
      });
      await openServerResult(result as string);
    }),
    commands.registerCommand("badjuju.describe.open", async () => {
      const args = cursorArgsForActiveEditor() ?? [];
      try {
        const result = await client.sendRequest("workspace/executeCommand", {
          command: "badjuju.describe",
          arguments: args,
        });
        const doc = await workspace.openTextDocument(
          Uri.parse(result as string),
        );
        await window.showTextDocument(doc, {
          preview: false,
          preserveFocus: false,
        });
      } catch (e) {
        window.showInformationMessage(`describe: ${(e as Error).message}`);
      }
    }),
    commands.registerCommand("badjuju.log.open", async () => {
      const defaultRevset: string =
        workspace.getConfiguration("badjuju").get("defaultLogRevset") ?? "";
      const result = await client.sendRequest("workspace/executeCommand", {
        command: "badjuju.log",
        arguments: defaultRevset ? [defaultRevset] : [],
      });
      const doc = await workspace.openTextDocument(Uri.parse(result as string));
      await window.showTextDocument(doc, { preserveFocus: false });
    }),
    commands.registerCommand("badjuju.log.applyShortcut", async () => {
      const editor = window.activeTextEditor;
      if (!editor) return;
      const cursorLine = editor.selection.active.line;
      const cursorChar = editor.selection.active.character;
      const doc = editor.document;

      const reloaded = waitForDocumentChange(doc, 1000);
      try {
        await client.sendRequest("workspace/executeCommand", {
          command: "badjuju.log",
          arguments: [
            { cursor: { uri: doc.uri.toString(), line: cursorLine } },
          ],
        });
      } catch (e) {
        window.showInformationMessage(`log: ${(e as Error).message}`);
        return;
      }
      await reloaded;

      const restoredLine = Math.min(cursorLine, doc.lineCount - 1);
      const restoredChar = Math.min(
        cursorChar,
        doc.lineAt(restoredLine).text.length,
      );
      const pos = new Position(restoredLine, restoredChar);
      editor.selection = new Selection(pos, pos);
      editor.revealRange(new Range(pos, pos), TextEditorRevealType.Default);
    }),
    window.onDidChangeTextEditorSelection((e) => {
      updateLogShortcutContext(e.textEditor);
    }),
    window.onDidChangeActiveTextEditor((editor) => {
      updateLogShortcutContext(editor);
    }),
    commands.registerCommand("badjuju.new.open", async () => {
      // When invoked from a status or log buffer, ship the cursor so the server
      // can resolve the commit under it; otherwise fall back to the server
      // default of creating a child of @.
      const args = cursorArgsForActiveEditor() ?? [];
      try {
        const result = await client.sendRequest("workspace/executeCommand", {
          command: "badjuju.new",
          arguments: args,
        });
        await openServerResult(result as string);
      } catch (e) {
        window.showInformationMessage(`new: ${(e as Error).message}`);
      }
    }),
    commands.registerCommand("badjuju.next.open", async () => {
      const result = await client.sendRequest("workspace/executeCommand", {
        command: "badjuju.next",
        arguments: [false],
      });
      await openServerResult(result as string);
    }),
    commands.registerCommand("badjuju.next.edit", async () => {
      const result = await client.sendRequest("workspace/executeCommand", {
        command: "badjuju.next",
        arguments: [true],
      });
      await openServerResult(result as string);
    }),
    commands.registerCommand("badjuju.prev.open", async () => {
      const result = await client.sendRequest("workspace/executeCommand", {
        command: "badjuju.prev",
        arguments: [false],
      });
      await openServerResult(result as string);
    }),
    commands.registerCommand("badjuju.prev.edit", async () => {
      const result = await client.sendRequest("workspace/executeCommand", {
        command: "badjuju.prev",
        arguments: [true],
      });
      await openServerResult(result as string);
    }),
    commands.registerCommand("badjuju.refresh.open", async () => {
      const activeDoc = window.activeTextEditor?.document.uri;
      const serverUri =
        activeDoc?.scheme === READONLY_SCHEME
          ? toFileUri(activeDoc).toString()
          : (activeDoc?.toString() ?? "");
      const result = await client.sendRequest("workspace/executeCommand", {
        command: "badjuju.refresh",
        arguments: [serverUri],
      });
      await openServerResult(result as string);
    }),
    commands.registerCommand("badjuju.squash.file", async () => {
      await runFileScopedStatusCommand("badjuju.squash");
    }),
    commands.registerCommand("badjuju.unsquash.file", async () => {
      await runFileScopedStatusCommand("badjuju.unsquash");
    }),
    commands.registerCommand("badjuju.toggleStat.open", async () => {
      const result = await client.sendRequest("workspace/executeCommand", {
        command: "badjuju.toggleStat",
        arguments: [],
      });
      await openServerResult(result as string);
    }),
    commands.registerCommand("badjuju.undo.open", async () => {
      const result = await client.sendRequest("workspace/executeCommand", {
        command: "badjuju.undo",
        arguments: [],
      });
      await openServerResult(result as string);
    }),
    commands.registerCommand("badjuju.diff.cursor", async () => {
      const args = cursorArgsForActiveEditor() ?? [];
      try {
        const result = await client.sendRequest("workspace/executeCommand", {
          command: "badjuju.diff",
          arguments: args,
        });
        await openServerResult(result as string, { aside: true });
      } catch (e) {
        window.showInformationMessage(`diff: ${(e as Error).message}`);
      }
    }),
    commands.registerCommand("badjuju.describe.finalize", async () => {
      await commands.executeCommand("workbench.action.files.save");
      await commands.executeCommand("workbench.action.closeActiveEditor");
    }),
    commands.registerCommand("badjuju.restartLanguageServer", async () => {
      await client.restart();
    }),
    commands.registerCommand("badjuju.version.open", async () => {
      const result = await client.sendRequest("workspace/executeCommand", {
        command: "badjuju.version",
        arguments: [],
      });
      const server = result as { version: string; commit: string };
      window.showInformationMessage(
        `Bad Juju  |  Client: v${__BADJUJU_VERSION__} (${__BADJUJU_COMMIT__})  |  Server: v${server.version} (${server.commit})`,
      );
    }),
    commands.registerCommand("badjuju.help.open", async () => {
      const editor = window.activeTextEditor;
      let windowType = "status";
      if (editor) {
        const uri = editor.document.uri;
        if (isLogFile(uri)) windowType = "log";
        else if (isDiffFile(uri)) windowType = "diff";
        else if (isDescribeFile(uri)) windowType = "describe";
      }
      const result = await client.sendRequest("workspace/executeCommand", {
        command: "badjuju.help",
        arguments: [windowType],
      });
      const entries = result as Array<{
        key: string;
        action: string;
        description: string;
      }>;
      if (!entries?.length) return;
      const items = entries
        .filter((e) => e.key)
        .map((e) => ({ label: e.key, description: e.description }));
      await window.showQuickPick(items, {
        title: `Bad Juju — ${windowType} bindings`,
        placeHolder: "Press Escape to close",
      });
    }),
    commands.registerCommand("badjuju.fetch.run", async () => {
      const result = await client.sendRequest("workspace/executeCommand", {
        command: "badjuju.fetch",
        arguments: [],
      });
      await openServerResult(result as string);
    }),
    commands.registerCommand("badjuju.push.normal", async () => {
      const result = await client.sendRequest("workspace/executeCommand", {
        command: "badjuju.push",
        arguments: [{ forceWithLease: false }],
      });
      await openServerResult(result as string);
    }),
    commands.registerCommand("badjuju.push.forceWithLease", async () => {
      const result = await client.sendRequest("workspace/executeCommand", {
        command: "badjuju.push",
        arguments: [{ forceWithLease: true }],
      });
      await openServerResult(result as string);
    }),
    commands.registerCommand("badjuju.rebase.prompt", async () => {
      const editor = window.activeTextEditor;
      let source = "@";
      if (editor) {
        const uri = editor.document.uri;
        const lines: string[] = [];
        for (let i = 0; i < editor.document.lineCount; i++) {
          lines.push(editor.document.lineAt(i).text);
        }
        const cursorLine = editor.selection.active.line;
        if (isStatusFile(uri)) {
          source = findRevisionForLine(lines, cursorLine);
        } else if (isLogFile(uri)) {
          const found = findLogRevision(lines, cursorLine);
          if (!found) {
            window.showInformationMessage(
              "rebase: place cursor on a commit line",
            );
            return;
          }
          source = found;
        }
      }
      const dest = await window.showInputBox({
        prompt: "Rebase to (destination revision):",
        placeHolder: "e.g. main, @-, abc1234",
      });
      if (!dest) return;
      const result = await client.sendRequest("workspace/executeCommand", {
        command: "badjuju.rebase",
        arguments: [source, dest],
      });
      await openServerResult(result as string);
    }),
    commands.registerCommand("badjuju.edit.cursor", async () => {
      const args = cursorArgsForActiveEditor() ?? [];
      try {
        const result = await client.sendRequest("workspace/executeCommand", {
          command: "badjuju.edit",
          arguments: args,
        });
        await openServerResult(result as string);
      } catch (e) {
        window.showInformationMessage(`edit: ${(e as Error).message}`);
      }
    }),
    commands.registerCommand("badjuju.abandon.cursor", async () => {
      // Capture whether we're in a log buffer before the abandon, so we can
      // stay there afterward (refresh log) instead of jumping to status.
      const editor = window.activeTextEditor;
      const logUri =
        editor && isLogFile(editor.document.uri)
          ? editor.document.uri.toString()
          : null;
      const args = cursorArgsForActiveEditor() ?? [];
      let result: unknown;
      try {
        result = await client.sendRequest("workspace/executeCommand", {
          command: "badjuju.abandon",
          arguments: args,
        });
      } catch (e) {
        window.showInformationMessage(`abandon: ${(e as Error).message}`);
        return;
      }
      if (logUri) {
        // Stay in the log view — refresh it instead of opening the returned status URI.
        const logResult = await client.sendRequest("workspace/executeCommand", {
          command: "badjuju.refresh",
          arguments: [logUri],
        });
        await openServerResult(logResult as string);
        return;
      }
      await openServerResult(result as string);
    }),
    commands.registerCommand("badjuju.bookmark.prompt", async () => {
      const SUB_ACTIONS = [
        {
          label: "create",
          description: "Create a new bookmark at cursor revision",
        },
        {
          label: "move",
          description: "Move an existing bookmark to cursor revision",
        },
        { label: "delete", description: "Delete a bookmark" },
        {
          label: "track",
          description: "Track a remote bookmark (e.g. main@origin)",
        },
        {
          label: "forget",
          description: "Forget a bookmark without recording deletion",
        },
      ];
      const picked = await window.showQuickPick(SUB_ACTIONS, {
        title: "jj bookmark — choose action",
        placeHolder: "Select a bookmark action",
      });
      if (!picked) return;

      // Determine the revision at cursor (used by create and move).
      const editor = window.activeTextEditor;
      let revision = "@";
      if (editor) {
        const uri = editor.document.uri;
        const lines: string[] = [];
        for (let i = 0; i < editor.document.lineCount; i++) {
          lines.push(editor.document.lineAt(i).text);
        }
        const cursorLine = editor.selection.active.line;
        if (isStatusFile(uri)) {
          revision = findRevisionForLine(lines, cursorLine);
        } else if (isLogFile(uri)) {
          const found = findLogRevision(lines, cursorLine);
          if (
            !found &&
            (picked.label === "create" || picked.label === "move")
          ) {
            window.showInformationMessage(
              `bookmark ${picked.label}: place cursor on a commit line`,
            );
            return;
          }
          revision = found ?? "@";
        }
      }

      const needsRev = picked.label === "create" || picked.label === "move";
      const namePrompt =
        picked.label === "track"
          ? "Bookmark name (e.g. main@origin):"
          : "Bookmark name:";
      const name = await window.showInputBox({ prompt: namePrompt });
      if (!name) return;

      const result = await client.sendRequest("workspace/executeCommand", {
        command: "badjuju.bookmark",
        arguments: [picked.label, name, needsRev ? revision : ""],
      });
      await openServerResult(result as string);
    }),
  );

  client = new LanguageClient(
    "badjuju",
    "Bad Juju",
    serverOptions,
    clientOptions,
  );
  client.start();
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) return undefined;
  return client.stop();
}
