import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import PermissionDialog, { PermissionKind, PermissionsStatus } from "./PermissionDialog";
import Setup, { ModelStatus } from "./Setup";
import Dashboard from "./Dashboard";
import { ShortcutStatus } from "./ShortcutSetting";
import type { AlertItem } from "./Alert";
import type { StatusLine } from "./TopBar";
import { APP_NAME } from "./appName";

// How long the empty pill stays out at the end of the first launch.
const ACTIVE_NOTICE_MS = 3000;

type DictationPhase = {
  phase: "idle" | "recording" | "transcribing" | "rewriting" | "pasting";
  error: string | null;
};

const PHASE_LABELS = {
  idle: "En attente",
  recording: "Enregistrement…",
  transcribing: "Transcription…",
  rewriting: "Reformulation…",
  pasting: "Collage…",
};

function App() {
  const [shortcut, setShortcut] = useState<ShortcutStatus | null>(null);
  const [learnShortcut, setLearnShortcut] = useState<ShortcutStatus | null>(null);
  const [permissions, setPermissions] = useState<PermissionsStatus | null>(null);
  const [dictation, setDictation] = useState<DictationPhase>({ phase: "idle", error: null });
  const [permError, setPermError] = useState<string | null>(null);
  const [model, setModel] = useState<ModelStatus>({ state: "loading", error: null });
  const [rewriteModel, setRewriteModel] = useState<ModelStatus>({ state: "loading", error: null });
  // null until Rust says whether a permission or a model is missing.
  const [setup, setSetup] = useState<boolean | null>(null);
  const [setupError, setSetupError] = useState<string | null>(null);
  // Permissions whose dialog the user closed: they stay as an alert in the top bar.
  const [dismissed, setDismissed] = useState<PermissionKind[]>([]);

  useEffect(() => {
    invoke<boolean>("needs_setup")
      .then(setSetup)
      .catch((err) => {
        setSetupError(String(err));
        setSetup(true);
      });
  }, []);

  useEffect(() => {
    invoke<ShortcutStatus>("get_shortcut_status").then(setShortcut);
    invoke<ShortcutStatus>("get_learn_shortcut_status").then(setLearnShortcut);
  }, []);

  useEffect(() => {
    invoke<ModelStatus>("get_model_status").then(setModel);
    invoke<ModelStatus>("get_rewrite_status").then(setRewriteModel);
  }, []);

  useEffect(() => {
    let cancelled = false;
    const unlisteners: (() => void)[] = [];
    const subscribe = <T,>(name: string, handler: (payload: T) => void) =>
      listen<T>(name, (event) => handler(event.payload)).then((fn) => {
        if (cancelled) fn();
        else unlisteners.push(fn);
      });
    subscribe<ModelStatus>("model-status", setModel);
    subscribe<ModelStatus>("rewrite-status", setRewriteModel);
    subscribe<DictationPhase>("dictation-phase", setDictation);
    return () => {
      cancelled = true;
      unlisteners.forEach((fn) => fn());
    };
  }, []);

  // The user grants the permission in System Settings, so we re-check regularly.
  useEffect(() => {
    const refresh = () =>
      invoke<PermissionsStatus>("get_permissions_status").then(setPermissions);
    refresh();
    const timer = setInterval(refresh, 1000);
    return () => clearInterval(timer);
  }, []);

  // The end of the first launch: the notch comes out empty for a few seconds, as at each start.
  const finishSetup = () => {
    setSetup(false);
    invoke("show_notice", { text: "", ms: ACTIVE_NOTICE_MS }).catch(() => {});
  };

  if (setup === null) return null;
  if (setup) {
    return (
      <>
        {setupError && <p className="setup-error">{setupError}</p>}
        <Setup
          shortcut={shortcut}
          onShortcutChange={setShortcut}
          learnShortcut={learnShortcut}
          onLearnShortcutChange={setLearnShortcut}
          transcription={model}
          rewriting={rewriteModel}
          permissions={permissions}
          onDone={finishSetup}
        />
      </>
    );
  }

  const modelsReady = model.state === "ready" && rewriteModel.state === "ready";
  const status: StatusLine =
    dictation.phase === "recording"
      ? { label: PHASE_LABELS.recording, tone: "danger" }
      : dictation.phase !== "idle"
        ? { label: PHASE_LABELS[dictation.phase], tone: "neutral" }
        : model.state === "failed"
          ? { label: "Transcription impossible", tone: "danger" }
          : !modelsReady
            ? { label: "Chargement des modèles…", tone: "neutral" }
            : { label: "Prêt à dicter", tone: "success" };

  const alerts: AlertItem[] = [];
  const missing: PermissionKind[] = [];
  if (permissions && !permissions.microphone) missing.push("microphone");
  if (permissions && !permissions.accessibility) missing.push("accessibility");
  // The first missing permission not yet dismissed is asked in a dialog; once dismissed, it is an alert.
  const dialogKind = missing.find((kind) => !dismissed.includes(kind));
  if (missing.includes("microphone") && dismissed.includes("microphone")) {
    alerts.push({
      key: "microphone",
      tone: "warning",
      title: "Micro non autorisé",
      text: `Sans lui, rien n'est enregistré. Si rien ne s'affiche, active ${APP_NAME} dans Réglages Système, Confidentialité et sécurité, Micro.`,
      action: {
        // Setup has already asked once, and macOS asks only once: from now on it is System Settings.
        label: "Ouvrir les réglages",
        run: () => invoke("open_microphone_settings").catch((e) => setPermError(String(e))),
      },
    });
  }
  if (missing.includes("accessibility") && dismissed.includes("accessibility")) {
    alerts.push({
      key: "accessibility",
      tone: "warning",
      title: "Accessibilité non autorisée",
      text: "Sans elle, le texte ne peut pas être collé.",
      action: { label: "Ouvrir les réglages", run: () => invoke("request_accessibility") },
    });
  }
  if (permError) alerts.push({ key: "permission-error", tone: "danger", title: permError });
  if (dictation.error) {
    // macOS delivers pure silence when the microphone is refused, and the permission check
    // can still say "granted" until Aède restarts: the error is the only sign, so it offers the fix.
    const silent = dictation.error.startsWith("Aucun son capté");
    // The microphone alert already says it, with the same button: one alert is enough.
    const duplicate = silent && alerts.some((alert) => alert.key === "microphone");
    if (!duplicate) {
      alerts.push({
        key: "dictation-error",
        tone: "danger",
        title: dictation.error,
        action: silent
          ? {
              label: "Ouvrir les réglages",
              run: () => invoke("open_microphone_settings").catch((e) => setPermError(String(e))),
            }
          : undefined,
      });
    }
  }
  if (model.error) alerts.push({ key: "model-error", tone: "danger", title: model.error });
  if (rewriteModel.error) {
    alerts.push({
      key: "rewrite-error",
      tone: "danger",
      title: rewriteModel.error,
      text: "Le texte est collé sans reformulation.",
    });
  }
  if (model.state === "ready" && rewriteModel.state === "loading") {
    alerts.push({
      key: "rewrite-loading",
      tone: "info",
      title: "Reformulation en cours de chargement",
      text: "Tant qu'elle n'est pas prête, la transcription brute est collée.",
    });
  }

  return (
    <>
      <Dashboard
        status={status}
        alerts={alerts}
        shortcut={shortcut}
        onShortcutChange={setShortcut}
        learnShortcut={learnShortcut}
        onLearnShortcutChange={setLearnShortcut}
        permissions={permissions}
      />
      {dialogKind && (
        <PermissionDialog
          kind={dialogKind}
          onClose={() => setDismissed((list) => [...list, dialogKind])}
          onError={setPermError}
        />
      )}
    </>
  );
}

export default App;
