import { promises as fs } from "node:fs";
import {
  commands,
  EventEmitter,
  type ExtensionContext,
  languages,
  type TextDocumentContentProvider,
  type TextEditor,
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

function isStatusFile(uri: Uri): boolean {
  return uri.path.endsWith("/status.jj");
}

function isLogFile(uri: Uri): boolean {
  return uri.path.endsWith("/log.jj");
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

export async function activate(context: ExtensionContext) {
  const traceOutputChannel = window.createOutputChannel(
    "Bad Juju - Jujutsu VCS",
  );
  const command = process.env.SERVER_PATH || "badjuju";
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
      const lineText = editor.document.lineAt(
        editor.selection.active.line,
      ).text;
      const match = lineText.match(LOG_SHORTCUT_LINE_RE);
      if (!match) return;
      const revset = match[2].trim();
      const result = await client.sendRequest("workspace/executeCommand", {
        command: "badjuju.log",
        arguments: [revset],
      });
      const doc = await workspace.openTextDocument(Uri.parse(result as string));
      await window.showTextDocument(doc, { preserveFocus: false });
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
