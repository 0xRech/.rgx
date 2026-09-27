(() => {
  "use strict";

  const tauri = window.__TAURI__;
  if (!tauri?.core?.invoke || !tauri?.dialog) {
    document.body.innerHTML =
      '<div style="padding:40px;font-family:sans-serif;color:white;background:#0b0c11;min-height:100vh">RGX Desktop muss innerhalb von Tauri gestartet werden.</div>';
    return;
  }

  const invoke = tauri.core.invoke;
  const dialog = tauri.dialog;

  const state = {
    view: "pack",
    packMode: "plain",
    recipientKeys: [],
  };

  const titles = {
    pack: "Archiv erstellen",
    extract: "Entpacken",
    inspect: "Prüfen & Info",
    identity: "Identity",
  };

  const $ = (id) => document.getElementById(id);
  const all = (selector) => Array.from(document.querySelectorAll(selector));

  function value(id) {
    return $(id).value.trim();
  }

  function secretValue(id) {
    return $(id).value;
  }

  function optional(valueToNormalize) {
    const normalized = valueToNormalize.trim();
    return normalized.length ? normalized : null;
  }

  function setStatus(text, kind = "ready") {
    $("status-text").textContent = text;
    const dot = $("status-dot");
    dot.classList.remove("busy", "error");
    if (kind === "busy") dot.classList.add("busy");
    if (kind === "error") dot.classList.add("error");
  }

  function toast(message, kind = "success") {
    const node = document.createElement("div");
    node.className = `toast ${kind}`;
    node.textContent = message;
    $("toast-container").appendChild(node);
    window.setTimeout(() => node.remove(), 4200);
  }

  function errorMessage(error) {
    if (typeof error === "string") return error;
    if (error && typeof error.message === "string") return error.message;
    return String(error ?? "Unbekannter Fehler");
  }

  async function runAction(label, action) {
    if (document.body.classList.contains("busy")) return null;

    document.body.classList.add("busy");
    setStatus(label, "busy");

    try {
      const result = await action();
      setStatus("Bereit");
      return result;
    } catch (error) {
      const message = errorMessage(error);
      setStatus("Fehler", "error");
      toast(message, "error");
      return null;
    } finally {
      document.body.classList.remove("busy");
    }
  }

  function switchView(view) {
    state.view = view;
    all(".nav-item").forEach((item) => {
      item.classList.toggle("active", item.dataset.view === view);
    });
    all(".view").forEach((panel) => {
      panel.classList.toggle("active", panel.id === `view-${view}`);
    });
    $("page-title").textContent = titles[view] ?? "RGX Desktop";
  }

  function setPackMode(mode) {
    state.packMode = mode;
    all("#pack-mode button").forEach((button) => {
      button.classList.toggle("active", button.dataset.mode === mode);
    });
    all(".mode-panel").forEach((panel) => {
      panel.classList.toggle("active", panel.id === `mode-${mode}`);
    });
  }

  function normalizeDialogPath(result) {
    if (typeof result === "string") return result;
    if (Array.isArray(result)) return result[0] ?? null;
    return null;
  }

  async function pickFile(filters = []) {
    return normalizeDialogPath(
      await dialog.open({
        multiple: false,
        directory: false,
        filters,
      }),
    );
  }

  async function pickFolder() {
    return normalizeDialogPath(
      await dialog.open({
        multiple: false,
        directory: true,
      }),
    );
  }

  async function pickIdentity() {
    return pickFile([
      {
        name: "RGX Identity",
        extensions: ["rgx", "key", "txt", "*"],
      },
    ]);
  }

  async function pickArchive() {
    return pickFile([{ name: "RGX Archive", extensions: ["rgx"] }]);
  }

  function renderRecipients() {
    const container = $("recipient-list");
    container.replaceChildren();

    if (!state.recipientKeys.length) {
      container.classList.add("empty");
      container.textContent = "Noch keine Empfänger ausgewählt.";
      return;
    }

    container.classList.remove("empty");
    state.recipientKeys.forEach((path, index) => {
      const chip = document.createElement("div");
      chip.className = "recipient-chip";

      const text = document.createElement("span");
      text.textContent = path;
      text.title = path;

      const remove = document.createElement("button");
      remove.type = "button";
      remove.textContent = "×";
      remove.title = "Empfänger entfernen";
      remove.addEventListener("click", () => {
        state.recipientKeys.splice(index, 1);
        renderRecipients();
      });

      chip.append(text, remove);
      container.appendChild(chip);
    });
  }

  function formatBytes(bytes) {
    const number = Number(bytes ?? 0);
    if (!Number.isFinite(number)) return "—";
    if (number < 1024) return `${number} B`;
    const units = ["KiB", "MiB", "GiB", "TiB"];
    let value = number;
    let unit = -1;
    do {
      value /= 1024;
      unit += 1;
    } while (value >= 1024 && unit < units.length - 1);
    const digits = value >= 100 ? 0 : value >= 10 ? 1 : 2;
    return `${value.toFixed(digits)} ${units[unit]}`;
  }

  function infoStats(info) {
    const items = [
      ["Dateien", info.files],
      ["Ordner", info.directories],
      ["Original", formatBytes(info.originalBytes)],
      ["Gespeichert", formatBytes(info.storedBytes)],
      ["Unique Chunks", info.uniqueChunks],
      ["Chunk-Refs", info.chunkReferences],
      ["Dedup gespart", formatBytes(info.deduplicatedBytes)],
      ["Format", `v${info.version}`],
    ];

    const stats = document.createElement("div");
    stats.className = "stats";

    items.forEach(([label, data]) => {
      const stat = document.createElement("div");
      stat.className = "stat";
      const labelNode = document.createElement("span");
      labelNode.textContent = label;
      const valueNode = document.createElement("strong");
      valueNode.textContent = String(data);
      stat.append(labelNode, valueNode);
      stats.appendChild(stat);
    });

    return stats;
  }

  function resultHeader(title, path, kind, successLabel = "OK") {
    const head = document.createElement("div");
    head.className = "result-head";

    const text = document.createElement("div");
    const h3 = document.createElement("h3");
    h3.textContent = title;
    const p = document.createElement("p");
    p.textContent = path;
    text.append(h3, p);

    const tagWrap = document.createElement("div");
    tagWrap.style.display = "flex";
    tagWrap.style.gap = "6px";

    if (kind) {
      const kindTag = document.createElement("span");
      kindTag.className = "kind-tag";
      kindTag.textContent = kind;
      tagWrap.appendChild(kindTag);
    }

    const success = document.createElement("span");
    success.className = "success-tag";
    success.textContent = successLabel;
    tagWrap.appendChild(success);

    head.append(text, tagWrap);
    return head;
  }

  function metaLine(items) {
    const line = document.createElement("div");
    line.className = "meta-line";
    items.filter(Boolean).forEach((item) => {
      const span = document.createElement("span");
      span.textContent = item;
      line.appendChild(span);
    });
    return line;
  }

  function renderOperationResult(container, title, response) {
    container.replaceChildren();
    container.classList.remove("hidden");
    container.append(
      resultHeader(title, response.output ?? response.archive ?? "", response.kind),
    );

    if (response.info) {
      container.appendChild(infoStats(response.info));
    }

    const meta = [];
    if (response.unlockMethod) meta.push(`Unlock: ${response.unlockMethod}`);
    if (response.recipientCount !== null && response.recipientCount !== undefined) {
      meta.push(`Recipients: ${response.recipientCount}`);
    }
    if (response.passwordFallback !== null && response.passwordFallback !== undefined) {
      meta.push(`Password fallback: ${response.passwordFallback ? "ja" : "nein"}`);
    }
    if (meta.length) container.appendChild(metaLine(meta));
  }

  function accessRequest(prefix) {
    return {
      archive: value(`${prefix}-archive`),
      identity: optional(value(`${prefix}-identity`)),
      archivePassword: optional(secretValue(`${prefix}-password`)),
      keyPassphrase: optional(secretValue(`${prefix}-key-passphrase`)),
    };
  }

  function requireValue(id, message) {
    const current = value(id);
    if (!current) throw new Error(message);
    return current;
  }

  function requireMatchingPasswords(firstId, secondId, message) {
    const first = secretValue(firstId);
    const second = secretValue(secondId);
    if (!first) throw new Error(message);
    if (first !== second) throw new Error("Die beiden Passwörter stimmen nicht überein.");
    return first;
  }

  function renderInspectInfo(response) {
    const container = $("inspect-result");
    container.replaceChildren();
    container.append(
      resultHeader("Archivinformationen", response.archive, response.kind, "GELESEN"),
      infoStats(response.info),
      metaLine([
        response.unlockMethod ? `Unlock: ${response.unlockMethod}` : null,
        response.recipientCount !== null
          ? `Recipients: ${response.recipientCount}`
          : null,
        response.passwordFallback !== null
          ? `Password fallback: ${response.passwordFallback ? "ja" : "nein"}`
          : null,
      ]),
    );
  }

  function renderVerify(response) {
    const container = $("inspect-result");
    container.replaceChildren();
    container.append(
      resultHeader("Integrität erfolgreich verifiziert", response.archive, response.kind),
    );

    const stats = document.createElement("div");
    stats.className = "stats";
    [
      ["Dateien", response.files],
      ["Unique Chunks", response.uniqueChunks],
      ["Chunk-Refs", response.chunkReferences],
      ["Status", "BLAKE3 / AEAD OK"],
    ].forEach(([label, data]) => {
      const stat = document.createElement("div");
      stat.className = "stat";
      const l = document.createElement("span");
      l.textContent = label;
      const v = document.createElement("strong");
      v.textContent = String(data);
      stat.append(l, v);
      stats.appendChild(stat);
    });

    container.appendChild(stats);
    if (response.unlockMethod) {
      container.appendChild(metaLine([`Unlock: ${response.unlockMethod}`]));
    }
  }

  function renderList(response) {
    const container = $("inspect-result");
    container.replaceChildren();

    const titlePath = value("inspect-archive");
    container.append(
      resultHeader(
        `Archivinhalt · ${response.total} Einträge`,
        titlePath,
        response.kind,
        "LISTE",
      ),
    );

    const wrap = document.createElement("div");
    wrap.className = "entries-wrap";

    const head = document.createElement("div");
    head.className = "entries-head";
    ["Typ", "Pfad", "Größe", "Chunks"].forEach((text) => {
      const cell = document.createElement("span");
      cell.textContent = text;
      head.appendChild(cell);
    });

    const list = document.createElement("div");
    list.className = "entries-list";

    response.entries.forEach((entry) => {
      const row = document.createElement("div");
      row.className = "entry-row";

      const type = document.createElement("span");
      type.className = "entry-type";
      type.textContent = entry.kind === "directory" ? "DIR" : "FILE";

      const path = document.createElement("span");
      path.className = "path";
      path.textContent = entry.path;
      path.title = entry.path;

      const size = document.createElement("span");
      size.textContent = entry.kind === "directory" ? "—" : formatBytes(entry.originalSize);

      const chunks = document.createElement("span");
      chunks.textContent = entry.kind === "directory" ? "—" : String(entry.chunks);

      row.append(type, path, size, chunks);
      list.appendChild(row);
    });

    wrap.append(head, list);
    container.appendChild(wrap);

    if (response.truncated) {
      const note = document.createElement("p");
      note.className = "list-note";
      note.textContent =
        "Zur Schonung der Oberfläche werden maximal 1.000 Einträge angezeigt.";
      container.appendChild(note);
    }

    if (response.unlockMethod) {
      container.appendChild(metaLine([`Unlock: ${response.unlockMethod}`]));
    }
  }

  all(".nav-item").forEach((button) => {
    button.addEventListener("click", () => switchView(button.dataset.view));
  });

  all("#pack-mode button").forEach((button) => {
    button.addEventListener("click", () => setPackMode(button.dataset.mode));
  });

  $("pack-level").addEventListener("input", (event) => {
    $("level-value").textContent = event.target.value;
  });

  $("pack-pick-file").addEventListener("click", async () => {
    const path = await pickFile();
    if (path) $("pack-input").value = path;
  });

  $("pack-pick-folder").addEventListener("click", async () => {
    const path = await pickFolder();
    if (path) $("pack-input").value = path;
  });

  $("pack-pick-output").addEventListener("click", async () => {
    const path = await dialog.save({
      defaultPath: "archive.rgx",
      filters: [{ name: "RGX Archive", extensions: ["rgx"] }],
    });
    if (path) $("pack-output").value = path;
  });

  $("recipient-add").addEventListener("click", async () => {
    const result = await dialog.open({
      multiple: true,
      directory: false,
      filters: [{ name: "RGX Public Key", extensions: ["pub"] }],
    });
    if (!result) return;
    const paths = Array.isArray(result) ? result : [result];
    for (const path of paths) {
      if (!state.recipientKeys.includes(path)) state.recipientKeys.push(path);
    }
    renderRecipients();
  });

  $("recipient-fallback").addEventListener("change", (event) => {
    $("recipient-fallback-fields").classList.toggle("hidden", !event.target.checked);
  });

  $("pack-run").addEventListener("click", async () => {
    const input = requireValue("pack-input", "Bitte zuerst eine Datei oder einen Ordner auswählen.");
    const output = requireValue("pack-output", "Bitte eine RGX-Zieldatei auswählen.");
    const level = Number($("pack-level").value);

    let password = null;
    let passwordFallback = false;

    if (state.packMode === "private") {
      password = requireMatchingPasswords(
        "private-password",
        "private-password-confirm",
        "Private Mode benötigt ein Passwort.",
      );
    }

    if (state.packMode === "recipient") {
      if (!state.recipientKeys.length) {
        throw new Error("Recipient Mode benötigt mindestens einen Public Key.");
      }
      passwordFallback = $("recipient-fallback").checked;
      if (passwordFallback) {
        password = requireMatchingPasswords(
          "recipient-password",
          "recipient-password-confirm",
          "Für den aktivierten Passwort-Fallback wird ein Passwort benötigt.",
        );
      }
    }

    const response = await runAction("RGX-Archiv wird erstellt …", () =>
      invoke("pack_archive", {
        request: {
          input,
          output,
          level,
          mode: state.packMode,
          password,
          recipientKeys: [...state.recipientKeys],
          passwordFallback,
        },
      }),
    );

    if (!response) return;

    renderOperationResult($("pack-result"), "Archiv erfolgreich erstellt", response);
    $("private-password").value = "";
    $("private-password-confirm").value = "";
    $("recipient-password").value = "";
    $("recipient-password-confirm").value = "";
    toast("RGX-Archiv wurde erfolgreich erstellt.");
  });

  $("extract-pick-archive").addEventListener("click", async () => {
    const path = await pickArchive();
    if (path) $("extract-archive").value = path;
  });

  $("extract-pick-parent").addEventListener("click", async () => {
    const path = await pickFolder();
    if (path) $("extract-parent").value = path;
  });

  $("extract-pick-identity").addEventListener("click", async () => {
    const path = await pickIdentity();
    if (path) $("extract-identity").value = path;
  });

  $("extract-clear-identity").addEventListener("click", () => {
    $("extract-identity").value = "";
    $("extract-key-passphrase").value = "";
  });

  $("extract-run").addEventListener("click", async () => {
    const archive = requireValue("extract-archive", "Bitte ein RGX-Archiv auswählen.");
    const outputParent = requireValue(
      "extract-parent",
      "Bitte einen Zielordner auswählen.",
    );

    const response = await runAction("RGX-Archiv wird entpackt …", () =>
      invoke("extract_archive", {
        request: {
          archive,
          outputParent,
          selectedPath: optional(value("extract-selected")),
          identity: optional(value("extract-identity")),
          archivePassword: optional(secretValue("extract-password")),
          keyPassphrase: optional(secretValue("extract-key-passphrase")),
        },
      }),
    );

    if (!response) return;

    renderOperationResult($("extract-result"), "Archiv erfolgreich entpackt", response);
    toast(`Entpackt nach: ${response.output}`);
  });

  $("inspect-pick-archive").addEventListener("click", async () => {
    const path = await pickArchive();
    if (path) $("inspect-archive").value = path;
  });

  $("inspect-pick-identity").addEventListener("click", async () => {
    const path = await pickIdentity();
    if (path) $("inspect-identity").value = path;
  });

  $("inspect-clear-identity").addEventListener("click", () => {
    $("inspect-identity").value = "";
    $("inspect-key-passphrase").value = "";
  });

  $("inspect-info").addEventListener("click", async () => {
    requireValue("inspect-archive", "Bitte ein RGX-Archiv auswählen.");
    const response = await runAction("Archivinformationen werden gelesen …", () =>
      invoke("inspect_archive", { request: accessRequest("inspect") }),
    );
    if (!response) return;
    renderInspectInfo(response);
    toast("Archivinformationen geladen.");
  });

  $("inspect-verify").addEventListener("click", async () => {
    requireValue("inspect-archive", "Bitte ein RGX-Archiv auswählen.");
    const response = await runAction("Archiv wird vollständig verifiziert …", () =>
      invoke("verify_archive", { request: accessRequest("inspect") }),
    );
    if (!response) return;
    renderVerify(response);
    toast("RGX-Integrität erfolgreich verifiziert.");
  });

  $("inspect-list").addEventListener("click", async () => {
    requireValue("inspect-archive", "Bitte ein RGX-Archiv auswählen.");
    const response = await runAction("Archivinhalt wird gelesen …", () =>
      invoke("list_archive", { request: accessRequest("inspect") }),
    );
    if (!response) return;
    renderList(response);
    toast("Archivinhalt geladen.");
  });

  $("identity-protect").addEventListener("change", (event) => {
    $("identity-password-fields").classList.toggle("hidden", !event.target.checked);
  });

  $("identity-pick-output").addEventListener("click", async () => {
    const path = await dialog.save({
      defaultPath: "id_rgx",
    });
    if (path) $("identity-output").value = path;
  });

  $("identity-run").addEventListener("click", async () => {
    const output = requireValue(
      "identity-output",
      "Bitte einen Speicherort für die RGX Identity auswählen.",
    );
    const protect = $("identity-protect").checked;
    let password = null;

    if (protect) {
      password = requireMatchingPasswords(
        "identity-password",
        "identity-password-confirm",
        "Eine geschützte Identity benötigt eine Passphrase.",
      );
    }

    const response = await runAction("RGX Identity wird erzeugt …", () =>
      invoke("generate_identity", {
        request: {
          output,
          protect,
          password,
        },
      }),
    );

    if (!response) return;

    const container = $("identity-result");
    container.replaceChildren();
    container.append(
      resultHeader(
        "Identity erfolgreich erzeugt",
        response.privateKey,
        response.protected ? "protected" : "passwordless",
      ),
      metaLine([
        `Public Key: ${response.publicKey}`,
        `Key-ID: ${response.keyId}`,
        response.protected
          ? "Private Key: Argon2id + XChaCha20-Poly1305"
          : "Private Key: Dateisystemschutz",
      ]),
    );

    $("identity-password").value = "";
    $("identity-password-confirm").value = "";
    toast("Neue RGX Identity wurde erzeugt.");
  });

  renderRecipients();
  setPackMode("plain");
  switchView("pack");
})();
