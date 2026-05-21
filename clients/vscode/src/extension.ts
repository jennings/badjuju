import { workspace, window, commands, Uri, ExtensionContext } from "vscode";
import {
  Executable,
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient;

export async function activate(context: ExtensionContext) {
  const traceOutputChannel = window.createOutputChannel("Bad Juju - Jujutsu VCS");
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

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "jujutsu" }],
    synchronize: {
      fileEvents: workspace.createFileSystemWatcher("**/.jj/**"),
    },
    traceOutputChannel,
  };

  context.subscriptions.push(
    commands.registerCommand("badjuju.status.open", async () => {
      const result = await client.sendRequest("workspace/executeCommand", {
        command: "badjuju.status",
        arguments: [],
      });
      const doc = await workspace.openTextDocument(Uri.parse(result as string));
      await window.showTextDocument(doc, { preserveFocus: false });
    }),
    commands.registerCommand("badjuju.describe.open", async () => {
      const result = await client.sendRequest("workspace/executeCommand", {
        command: "badjuju.describe",
        arguments: [],
      });
      const doc = await workspace.openTextDocument(Uri.parse(result as string));
      await window.showTextDocument(doc, { preview: false, preserveFocus: false });
    }),
    commands.registerCommand("badjuju.log.open", async () => {
      const result = await client.sendRequest("workspace/executeCommand", {
        command: "badjuju.log",
        arguments: [],
      });
      const doc = await workspace.openTextDocument(Uri.parse(result as string));
      await window.showTextDocument(doc, { preserveFocus: false });
    })
  );

  client = new LanguageClient("badjuju", "Bad Juju", serverOptions, clientOptions);
  client.start();
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) return undefined;
  return client.stop();
}
