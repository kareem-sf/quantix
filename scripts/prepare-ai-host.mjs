import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));

// An incremental application build, run only when the engineer starts Quantix.
// It does not install any provider software, sign in, or make model requests.
export async function prepareAIHost() {
  await new Promise((resolve, reject) => {
    const child = spawn(
      "cargo",
      [
        "build",
        "--manifest-path",
        path.join(root, "src-tauri", "Cargo.toml"),
        "--bin",
        "quantix-ai-host",
      ],
      {
        cwd: root,
        stdio: "inherit",
        windowsHide: true,
      },
    );
    child.once("error", () =>
      reject(
        new Error(
          "Quantix could not build its AI launcher. Install the Rust build tools used by this development application.",
        ),
      ),
    );
    child.once("exit", (code) =>
      code === 0
        ? resolve()
        : reject(
            new Error(
              "Quantix's AI launcher build could not finish. Read the build message above before restarting.",
            ),
          ),
    );
  });
}
