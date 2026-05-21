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

/** Parse a status.jj line and return the file path it refers to, or null. */
export function parseStatusFile(line: string): string | null {
  const m = line.match(STATUS_FILE_LINE_RE) ?? line.match(STAT_LINE_RE);
  if (!m) return null;
  const rest = m[m.length - 1].trim();
  // jj renders renames/copies as "old => new" — squash needs the destination path.
  const arrow = rest.lastIndexOf(" => ");
  return arrow >= 0 ? rest.slice(arrow + 4).trim() : rest;
}

function isStatusFile(uri: Uri): boolean {
  return uri.path.endsWith("/status.jj");
}

function isLogFile(uri: Uri): boolean {
  return uri.path.endsWith("/log.jj");
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

async function openServerResult(resultUri: string): Promise<void> {
  const parsed = Uri.parse(resultUri);
  if (parsed.scheme === "file" && isStatusFile(parsed)) {
    const readonlyUri = toReadonlyUri(parsed);
    statusProvider.refresh(readonlyUri);
    const doc = await workspace.openTextDocument(readonlyUri);
    await languages.setTextDocumentLanguage(doc, "jujutsu");
    await window.showTextDocument(doc, { preserveFocus: false });
    return;
  }
  const doc = await workspace.openTextDocument(parsed);
  await window.showTextDocument(doc, { preserveFocus: false });
}

async function runFileScopedStatusCommand(
  serverCommand: string,
): Promise<void> {
  const editor = window.activeTextEditor;
  if (!editor) return;
  if (!isStatusFile(editor.document.uri)) return;
  const lineText = editor.document.lineAt(editor.selection.active.line).text;
  const file = parseStatusFile(lineText);
  if (!file) {
    window.showInformationMessage(
      `${serverCommand}: place cursor on a changed file line`,
    );
    return;
  }
  const result = await client.sendRequest("workspace/executeCommand", {
    command: serverCommand,
    arguments: [file],
  });
  await openServerResult(result as string);
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
  const initializationOptions = binaryPath ? { binaryPath } : undefined;

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

  context.subscriptions.push(
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
      const result = await client.sendRequest("workspace/executeCommand", {
        command: "badjuju.describe",
        arguments: [],
      });
      const doc = await workspace.openTextDocument(Uri.parse(result as string));
      await window.showTextDocument(doc, {
        preview: false,
        preserveFocus: false,
      });
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
      const lineText = doc.lineAt(cursorLine).text;
      const match = lineText.match(LOG_SHORTCUT_LINE_RE);
      if (!match) return;
      const revset = match[2].trim();

      const reloaded = waitForDocumentChange(doc, 1000);
      await client.sendRequest("workspace/executeCommand", {
        command: "badjuju.log",
        arguments: [revset],
      });
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
      const result = await client.sendRequest("workspace/executeCommand", {
        command: "badjuju.new",
        arguments: [],
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
