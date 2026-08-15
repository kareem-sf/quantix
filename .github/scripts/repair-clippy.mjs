import { readFileSync, writeFileSync } from "node:fs";

function readNormalized(path) {
  return readFileSync(path, "utf8").replace(/\r\n/g, "\n");
}

function replaceOnce(content, before, after, label) {
  const count = content.split(before).length - 1;
  if (count !== 1) {
    throw new Error(`Expected one ${label} match, found ${count}.`);
  }
  return content.replace(before, after);
}

const actorPath = "src-tauri/src/agent_runtime/codex_actor.rs";
let actor = readNormalized(actorPath);
actor = replaceOnce(
  actor,
  "\nasync fn handle_message(\n",
  "\n#[allow(clippy::too_many_arguments)]\nasync fn handle_message(\n",
  "handle_message declaration",
);
actor = replaceOnce(
  actor,
  `            } else if result.get("status").and_then(Value::as_str) != Some("canceled") {
                if active_login_id.as_deref() == Some(login_id.as_str()) {
                    if let Some(current) = login
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .as_mut()
                    {
                        current.status = ProviderLoginStatus::Cancelling;
                        current.status_summary =
                            "Waiting for Codex to report the final login state.".to_owned();
                    }
                }
            }`,
  `            } else if result.get("status").and_then(Value::as_str) != Some("canceled")
                && active_login_id.as_deref() == Some(login_id.as_str())
            {
                if let Some(current) = login
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .as_mut()
                {
                    current.status = ProviderLoginStatus::Cancelling;
                    current.status_summary =
                        "Waiting for Codex to report the final login state.".to_owned();
                }
            }`,
  "login cancellation condition",
);
writeFileSync(actorPath, actor, "utf8");

const gatePath = "src-tauri/src/release_gate.rs";
let gate = readNormalized(gatePath);
gate = replaceOnce(
  gate,
  "components.get(0) == Some(&10)",
  "components.first() == Some(&10)",
  "Windows version first component",
);
writeFileSync(gatePath, gate, "utf8");

const acceptancePath = "src-tauri/src/bin/product_acceptance.rs";
let acceptance = readNormalized(acceptancePath);
acceptance = replaceOnce(
  acceptance,
  "components.get(0) != Some(&10)",
  "components.first() != Some(&10)",
  "acceptance Windows version first component",
);
writeFileSync(acceptancePath, acceptance, "utf8");

const releasePath = "src-tauri/src/tender_store/final_release.rs";
let release = readNormalized(releasePath);
release = replaceOnce(
  release,
  "value.as_bytes().len() > MAX_FINAL_TEXT_BYTES",
  "value.len() > MAX_FINAL_TEXT_BYTES",
  "final text byte length",
);
writeFileSync(releasePath, release, "utf8");
