import { existsSync, promises as fs } from "node:fs";
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
import { restoreCursor } from "./lib/clamp";
import { resolveServerCommand } from "./lib/serverPath";
import {
  DIFF_SCHEME,
  FILE_SCHEME,
  isLogFile,
  isReadonlyOutput,
  isSquashFile,
  isStatusFile,
  READONLY_SCHEME,
} from "./lib/uri";
import { windowTypeForUri } from "./lib/windowType";

declare const __BADJUJU_COMMIT__: string;
declare const __BADJUJU_VERSION__: string;

let client: LanguageClient;

function setPendingSquash(value: boolean): void {
  commands.executeCommand("setContext", "badjuju.pendingSquash", value);
}

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

class DiffContentProvider implements TextDocumentContentProvider {
  private readonly _onDidChange = new EventEmitter<Uri>();
  readonly onDidChange = this._onDidChange.event;

  async provideTextDocumentContent(uri: Uri): Promise<string> {
    try {
      const result = await client.sendRequest<{ text: string }>(
        "workspace/textDocumentContent",
        { uri: uri.toString() },
      );
      return result?.text ?? "";
    } catch {
      return "";
    }
  }

  refresh(uri: Uri): void {
    this._onDidChange.fire(uri);
  }
}

const diffProvider = new DiffContentProvider();

class FileContentProvider implements TextDocumentContentProvider {
  private readonly _onDidChange = new EventEmitter<Uri>();
  readonly onDidChange = this._onDidChange.event;

  async provideTextDocumentContent(uri: Uri): Promise<string> {
    try {
      const result = await client.sendRequest<{ text: string }>(
        "workspace/textDocumentContent",
        { uri: uri.toString() },
      );
      return result?.text ?? "";
    } catch {
      return "";
    }
  }
}

const fileProvider = new FileContentProvider();

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
  if (parsed.scheme === DIFF_SCHEME) {
    // Virtual diff: served via DiffContentProvider (no file on disk).
    diffProvider.refresh(parsed);
    const doc = await workspace.openTextDocument(parsed);
    await languages.setTextDocumentLanguage(doc, "jujutsu");
    await window.showTextDocument(doc, showOpts);
    return;
  }
  if (parsed.scheme === "file" && isReadonlyOutput(parsed)) {
    const readonlyUri = toReadonlyUri(parsed);
    statusProvider.refresh(readonlyUri);
    const doc = await workspace.openTextDocument(readonlyUri);
    await languages.setTextDocumentLanguage(doc, "jujutsu");
    await window.showTextDocument(doc, showOpts);
    return;
  }
  if (parsed.scheme === "file" && isSquashFile(parsed)) {
    const doc = await workspace.openTextDocument(parsed);
    await languages.setTextDocumentLanguage(doc, "jujutsu");
    await window.showTextDocument(doc, { ...showOpts, preview: false });
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
  // The server resolves cursor URIs via disk read; convert readonly scheme to
  // file:// so read_uri_from_disk can read the content without needing the
  // LSP document cache (which may not yet have the virtual document).
  const fileUri = uri.scheme === READONLY_SCHEME ? toFileUri(uri) : uri;
  return [
    {
      cursor: {
        uri: fileUri.toString(),
        line: editor.selection.active.line,
      },
    },
  ];
}

type RevisionArg = string | { cursor: { uri: string; line: number } };

/**
 * Prompt for a rebase destination and execute `badjuju.rebase`. Shared
 * between the `badjuju.rebase.prompt` hotkey (which ships a cursor-form
 * source) and the `badjuju.client.rebasePrompt` code-action command (which
 * receives a pre-resolved source string from the server).
 */
async function runRebasePrompt(source: RevisionArg): Promise<void> {
  const dest = await window.showInputBox({
    prompt: "Rebase to (destination revision):",
    placeHolder: "e.g. main, @-, abc1234",
  });
  if (!dest) return;
  try {
    const result = await client.sendRequest("workspace/executeCommand", {
      command: "badjuju.rebase",
      arguments: [source, dest],
    });
    await openServerResult(result as string);
  } catch (e) {
    window.showInformationMessage(`rebase: ${(e as Error).message}`);
  }
}

/**
 * Prompt for a bookmark name and execute `badjuju.bookmark`. Shared between
 * the `badjuju.bookmark.prompt` hotkey and the `badjuju.client.bookmarkPrompt`
 * code-action command. `subAction` is one of create/move/delete/track/forget.
 */
async function runBookmarkPrompt(
  subAction: string,
  revision: RevisionArg,
): Promise<void> {
  const needsRev = subAction === "create" || subAction === "move";
  const namePrompt =
    subAction === "track"
      ? "Bookmark name (e.g. main@origin):"
      : "Bookmark name:";
  const name = await window.showInputBox({ prompt: namePrompt });
  if (!name) return;
  try {
    const result = await client.sendRequest("workspace/executeCommand", {
      command: "badjuju.bookmark",
      arguments: [subAction, name, needsRev ? revision : ""],
    });
    await openServerResult(result as string);
  } catch (e) {
    window.showInformationMessage(`bookmark: ${(e as Error).message}`);
  }
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

export async function activate(context: ExtensionContext) {
  const traceOutputChannel = window.createOutputChannel(
    "Bad Juju - Jujutsu VCS",
  );
  const command = resolveServerCommand({
    envServerPath: process.env.SERVER_PATH,
    extensionPath: context.extensionPath,
    platform: process.platform,
    existsSync,
  });
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
  // Signal to the server that this client supports workspace/textDocumentContent
  // so it returns virtual badjuju-diff: URIs instead of writing files to disk.
  const initializationOptions: Record<string, unknown> = {
    keymapProfile,
    virtualDiffs: true,
  };
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
      { scheme: DIFF_SCHEME, language: "jujutsu" },
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
    // Fold all regions when a status buffer is first shown.
    // The delay lets the LSP publish its folding ranges before we collapse them.
    workspace.onDidOpenTextDocument(async (doc) => {
      if (!isStatusFile(doc.uri)) return;
      await new Promise<void>((resolve) => setTimeout(resolve, 200));
      const editor = window.visibleTextEditors.find(
        (e) => e.document.uri.toString() === doc.uri.toString(),
      );
      if (editor) {
        await window.showTextDocument(editor.document, {
          preserveFocus: true,
          viewColumn: editor.viewColumn,
        });
        await commands.executeCommand("editor.foldAll");
      }
    }),
    workspace.registerTextDocumentContentProvider(
      READONLY_SCHEME,
      statusProvider,
    ),
    workspace.registerTextDocumentContentProvider(DIFF_SCHEME, diffProvider),
    workspace.registerTextDocumentContentProvider(FILE_SCHEME, fileProvider),
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
      await openServerResult(result as string);
    }),
    commands.registerCommand("badjuju.log.prompt", async () => {
      const revset = await window.showInputBox({
        prompt: "Custom revset for log",
        placeHolder: "e.g. main, @-, all(), trunk()::",
      });
      if (revset === undefined) return;
      try {
        const result = await client.sendRequest("workspace/executeCommand", {
          command: "badjuju.log",
          arguments: [revset],
        });
        await openServerResult(result as string);
      } catch (e) {
        window.showInformationMessage(`log: ${(e as Error).message}`);
      }
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

      const { line: restoredLine, char: restoredChar } = restoreCursor({
        cursorLine,
        cursorChar,
        lineCount: doc.lineCount,
        lineLength: (i) => doc.lineAt(i).text.length,
      });
      const pos = new Position(restoredLine, restoredChar);
      editor.selection = new Selection(pos, pos);
      editor.revealRange(new Range(pos, pos), TextEditorRevealType.Default);
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
      const editor = window.activeTextEditor;
      if (!editor) return;
      if (!isStatusFile(editor.document.uri)) return;
      const cursorArg = {
        cursor: {
          uri: editor.document.uri.toString(),
          line: editor.selection.active.line,
        },
      };
      let result: unknown;
      try {
        result = await client.sendRequest("workspace/executeCommand", {
          command: "badjuju.squash",
          arguments: [cursorArg],
        });
      } catch (e) {
        // biome-ignore lint/suspicious/noExplicitAny: jsonrpc error data has no stable type
        const data = (e as any)?.data;
        if (data?.code === "RequiresParentSelection") {
          const candidates: Array<{ id: string; label: string }> =
            data.candidates;
          const picked = await window.showQuickPick(
            candidates.map((c) => ({ label: c.label, id: c.id })),
            { placeHolder: "Select parent to squash into" },
          );
          if (!picked) return;
          try {
            const result2 = await client.sendRequest(
              "workspace/executeCommand",
              {
                command: "badjuju.squash.into",
                arguments: [{ file: data.file, parentId: picked.id }],
              },
            );
            await openServerResult(result2 as string);
          } catch (e2) {
            window.showInformationMessage(
              `squash.into: ${(e2 as Error).message}`,
            );
          }
        } else {
          window.showInformationMessage(`squash: ${(e as Error).message}`);
        }
        return;
      }
      await openServerResult(result as string);
    }),
    commands.registerCommand("badjuju.unsquash.file", async () => {
      await runFileScopedStatusCommand("badjuju.unsquash");
    }),
    // Client-side wrappers around the server's `badjuju.squash.*` commands.
    // The wrapper name MUST differ from the server name: vscode-languageclient's
    // ExecuteCommandFeature auto-registers every server command as a VS Code
    // command, so reusing the same name throws "command already exists" at
    // initialize and tears the LSP connection down.
    commands.registerCommand("badjuju.squash.commit.cursor", async () => {
      const args = cursorArgsForActiveEditor() ?? [];
      try {
        const result = await client.sendRequest("workspace/executeCommand", {
          command: "badjuju.squash.commit",
          arguments: args,
        });
        const resultUri = result as string;
        // Source selection returns the status URI; destination selection returns the squash URI.
        if (isSquashFile(Uri.parse(resultUri))) {
          setPendingSquash(false);
        } else {
          setPendingSquash(true);
        }
        await openServerResult(resultUri);
      } catch (e) {
        window.showInformationMessage(`squash.commit: ${(e as Error).message}`);
      }
    }),
    commands.registerCommand("badjuju.squash.cancel.run", async () => {
      try {
        const result = await client.sendRequest("workspace/executeCommand", {
          command: "badjuju.squash.cancel",
          arguments: [],
        });
        setPendingSquash(false);
        await openServerResult(result as string);
      } catch (e) {
        window.showInformationMessage(`squash.cancel: ${(e as Error).message}`);
      }
    }),
    commands.registerCommand("badjuju.squash.toggle.cursor", async () => {
      const editor = window.activeTextEditor;
      if (!editor) return;
      try {
        await client.sendRequest("workspace/executeCommand", {
          command: "badjuju.squash.toggle",
          arguments: [
            {
              cursor: {
                uri: editor.document.uri.toString(),
                line: editor.selection.active.line,
              },
            },
          ],
        });
      } catch (e) {
        window.showInformationMessage(`squash.toggle: ${(e as Error).message}`);
      }
    }),
    commands.registerCommand("badjuju.squash.select_all.run", async () => {
      try {
        await client.sendRequest("workspace/executeCommand", {
          command: "badjuju.squash.select_all",
          arguments: [],
        });
      } catch (e) {
        window.showInformationMessage(
          `squash.select_all: ${(e as Error).message}`,
        );
      }
    }),
    commands.registerCommand("badjuju.squash.select_none.run", async () => {
      try {
        await client.sendRequest("workspace/executeCommand", {
          command: "badjuju.squash.select_none",
          arguments: [],
        });
      } catch (e) {
        window.showInformationMessage(
          `squash.select_none: ${(e as Error).message}`,
        );
      }
    }),
    commands.registerCommand("badjuju.squash.edit_hunk.cursor", async () => {
      const editor = window.activeTextEditor;
      if (!editor) return;
      try {
        const result = await client.sendRequest("workspace/executeCommand", {
          command: "badjuju.squash.edit_hunk",
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
      } catch (e) {
        window.showInformationMessage(
          `squash.edit_hunk: ${(e as Error).message}`,
        );
      }
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
    commands.registerCommand("badjuju.diff.cursor.commit", async () => {
      const args = cursorArgsForActiveEditor() ?? [];
      try {
        const result = await client.sendRequest("workspace/executeCommand", {
          command: "badjuju.diff.commit",
          arguments: args,
        });
        await openServerResult(result as string, { aside: true });
      } catch (e) {
        window.showInformationMessage(`diff (commit): ${(e as Error).message}`);
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
    commands.registerCommand("badjuju.ret.dispatch", async () => {
      const editor = window.activeTextEditor;
      if (!editor) return;
      const line = editor.document.lineAt(editor.selection.active.line).text;
      if (/^JJ: [^:]+:/.test(line)) {
        await commands.executeCommand("badjuju.log.applyShortcut");
      } else {
        await commands.executeCommand("editor.action.revealDefinition");
      }
    }),
    commands.registerCommand("badjuju.help.open", async () => {
      const editor = window.activeTextEditor;
      const windowType = windowTypeForUri(editor?.document.uri);
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
      const cursorArgs = cursorArgsForActiveEditor();
      // Cursor-form when in a jujutsu buffer; literal "@" otherwise. Server
      // returns an LSP error if the cursor isn't on a commit line.
      const source: RevisionArg = cursorArgs ? cursorArgs[0] : "@";
      await runRebasePrompt(source);
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

      const cursorArgs = cursorArgsForActiveEditor();
      const revision: RevisionArg = cursorArgs ? cursorArgs[0] : "@";
      await runBookmarkPrompt(picked.label, revision);
    }),
    // Client-side commands invoked by server-provided code actions. The server
    // ships {command: "badjuju.client.rebasePrompt", arguments: [<revision>]}
    // when a code action would need a destination/name that the server can't
    // resolve on its own; the handler prompts and forwards to the server cmd.
    commands.registerCommand(
      "badjuju.client.rebasePrompt",
      async (revision: string) => {
        await runRebasePrompt(revision ?? "@");
      },
    ),
    commands.registerCommand(
      "badjuju.client.bookmarkPrompt",
      async (revision: string) => {
        const SUB_ACTIONS = [
          {
            label: "create",
            description: "Create a new bookmark at this revision",
          },
          {
            label: "move",
            description: "Move an existing bookmark to this revision",
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
        await runBookmarkPrompt(picked.label, revision ?? "@");
      },
    ),
  );

  client = new LanguageClient(
    "badjuju",
    "Bad Juju",
    serverOptions,
    clientOptions,
  );
  client.start();

  // Subscribe to server-sent workspace/textDocumentContent/refresh.
  // The server fires this for each open change-mode diff when the underlying
  // change is mutated (describe save, squash, etc.).
  context.subscriptions.push(
    client.onNotification(
      "workspace/textDocumentContent/refresh",
      (params: { uri: string }) => {
        const uri = Uri.parse(params.uri);
        diffProvider.refresh(uri);
      },
    ),
  );
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) return undefined;
  return client.stop();
}
