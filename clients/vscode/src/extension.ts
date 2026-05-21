import { workspace, window, commands, ExtensionContext } from "vscode";
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
      const folders = workspace.workspaceFolders;
      if (!folders?.length) return;
      const uri = folders[0].uri.with({
        path: folders[0].uri.path + "/.jj/badjuju/status.jj",
      });
      const doc = await workspace.openTextDocument(uri);
      await window.showTextDocument(doc);
    })
  );

  client = new LanguageClient("badjuju", "Bad Juju", serverOptions, clientOptions);
  client.start();
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) return undefined;
  return client.stop();
}
