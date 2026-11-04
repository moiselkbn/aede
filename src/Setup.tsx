import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { APP_NAME } from "./appName";
import type { PermissionsStatus } from "./PermissionDialog";
import { DemoCards, DemoText } from "./setup/DemoScreen";
import { LearnCards, LearnText } from "./setup/LearnScreen";
import { MicTestCards, MicTestText } from "./setup/MicTestScreen";
import { ModelsCards, ModelsText } from "./setup/ModelsScreen";
import { PermissionsCards, PermissionsText } from "./setup/PermissionsScreen";
import { ReadyCards, ReadyText } from "./setup/ReadyScreen";
import SetupFrame from "./setup/SetupFrame";
import { ShortcutCards, ShortcutText } from "./setup/ShortcutScreen";
import type { ToastData } from "./setup/Toast";
import WelcomeDemo from "./setup/WelcomeDemo";
import { WelcomeText } from "./setup/WelcomeScreen";
import { formatProgress, formatSpeed } from "./setup/format";
import { useDictationDemo } from "./setup/useDictationDemo";
import { useLearnDemo } from "./setup/useLearnDemo";
import { useMicTest } from "./setup/useMicTest";
import { useModels } from "./setup/useModels";
import { useShortcutTest } from "./setup/useShortcutTest";
import { useSpeed } from "./setup/useSpeed";
import { useWindowShown } from "./setup/useWindowShown";
import { SHORTCUT_HOLD_MS, type ShortcutStatus, useShortcutCapture } from "./useShortcutCapture";
import "./setup/setup.css";

export type ModelStatus = { state: "loading" | "ready" | "failed"; error: string | null };

type Props = {
  /** The dictation shortcut: its label, and why it could not be registered if so. */
  shortcut: ShortcutStatus | null;
  onShortcutChange: (status: ShortcutStatus) => void;
  /** The learning shortcut, the same way. */
  learnShortcut: ShortcutStatus | null;
  onLearnShortcutChange: (status: ShortcutStatus) => void;
  /** Whether each model is loaded in memory, not only on disk. */
  transcription: ModelStatus;
  rewriting: ModelStatus;
  /** Null until the first answer of the permission check (App asks every second). */
  permissions: PermissionsStatus | null;
  onDone: () => void;
};

const NOTICE_MS = 10000;

/** The first launch: one screen at a time under the stepper, with the notices in the right panel. */
function Setup({
  shortcut,
  onShortcutChange,
  learnShortcut,
  onLearnShortcutChange,
  transcription,
  rewriting,
  permissions,
  onDone,
}: Props) {
  const models = useModels();
  const [step, setStep] = useState<"welcome" | "permissions" | "models" | "mic" | "shortcut" | "demo" | "learn" | "learnDemo" | "ready">("welcome");
  // "Commencer" is the user's go-ahead for the download: it starts once the list of models is known.
  const [begun, setBegun] = useState(false);
  const [notices, setNotices] = useState<ToastData[]>([]);
  const before = useRef<PermissionsStatus | null>(null);
  const timers = useRef<Record<string, ReturnType<typeof setTimeout>>>({});

  const closeNotice = (key: string) => setNotices((list) => list.filter((n) => n.key !== key));
  const addNotice = (notice: Omit<ToastData, "onClose">) => {
    setNotices((list) => [
      ...list.filter((n) => n.key !== notice.key),
      { ...notice, onClose: () => closeNotice(notice.key) },
    ]);
    // The same notice shown again restarts its 10 seconds.
    clearTimeout(timers.current[notice.key]);
    timers.current[notice.key] = setTimeout(() => closeNotice(notice.key), NOTICE_MS);
  };

  // The close button only hides the window: what the screens listen to stops while it is hidden,
  // and starts again when it comes back.
  const shown = useWindowShown();

  // The real notch stays off while these screens are on view: the window draws its own.
  useEffect(() => {
    if (!shown) return;
    invoke("set_setup_open", { on: true }).catch(() => {});
    return () => {
      invoke("set_setup_open", { on: false }).catch(() => {});
    };
  }, [shown]);

  // A permission that goes from missing to granted while the screen is open: a notice says what it allows.
  useEffect(() => {
    if (!permissions) return;
    const old = before.current;
    before.current = permissions;
    if (!old) return;
    if (!old.microphone && permissions.microphone) {
      closeNotice("microphone-refused");
      addNotice({
        key: "microphone",
        tone: "success",
        title: "Micro autorisé",
        text: `${APP_NAME} pourra enregistrer ta voix pendant que tu maintiens le raccourci.`,
      });
    }
    if (!old.accessibility && permissions.accessibility) {
      addNotice({
        key: "accessibility",
        tone: "success",
        title: "Accessibilité autorisée",
        text: `${APP_NAME} pourra coller le texte là où se trouve ton curseur.`,
      });
    }
  }, [permissions]);

  // Bytes on disk over both models, for the notice (the speed uses the bytes moved since the start).
  const total = models.models?.reduce((sum, m) => sum + m.size, 0) ?? 0;
  const received =
    models.models?.reduce(
      (sum, m) => sum + (m.present ? m.size : (models.progress[m.file_name]?.downloaded ?? 0)),
      0,
    ) ?? 0;
  const speed = useSpeed(models.transferred, models.downloading && !models.waiting);

  const loaded = transcription.state === "ready" && rewriting.state === "ready";
  const ready = models.modelsReady && loaded;
  const wasReady = useRef(ready);
  // Models already on disk at the start are not news: the notice is for a download seen here.
  const sawDownload = useRef(false);
  if (models.downloading) sawDownload.current = true;
  const micTest = useMicTest(step === "mic" && shown);
  const shortcutTest = useShortcutTest(step === "shortcut" && shown);
  const { reset: resetShortcutTest } = shortcutTest;
  // A new combination has to be tried again.
  const changeShortcut = useCallback(
    (status: ShortcutStatus) => {
      onShortcutChange(status);
      resetShortcutTest();
    },
    [onShortcutChange, resetShortcutTest],
  );
  const shortcutCapture = useShortcutCapture("set_shortcut", changeShortcut, {
    holdMs: SHORTCUT_HOLD_MS,
  });
  const learnTest = useShortcutTest(step === "learn" && shown, "learn");
  const { reset: resetLearnTest } = learnTest;
  const changeLearnShortcut = useCallback(
    (status: ShortcutStatus) => {
      onLearnShortcutChange(status);
      resetLearnTest();
    },
    [onLearnShortcutChange, resetLearnTest],
  );
  const learnCapture = useShortcutCapture("set_learn_shortcut", changeLearnShortcut, {
    holdMs: SHORTCUT_HOLD_MS,
  });
  const demoField = useRef<HTMLTextAreaElement>(null);
  const learnDemo = useLearnDemo(step === "learnDemo" && shown, learnShortcut?.label ?? "");
  // The trial needs both models loaded: the field stays closed until then.
  const demo = useDictationDemo(step === "demo" && ready && shown, demoField);

  // The models become ready while the user does something else (the microphone test): a notice says so.
  useEffect(() => {
    if (ready && !wasReady.current && sawDownload.current) {
      addNotice({
        key: "models-ready",
        tone: "success",
        title: "Les modèles sont prêts",
        text: `${APP_NAME} peut dicter dans n'importe quelle application.`,
      });
    }
    wasReady.current = ready;
  }, [ready]);

  useEffect(() => {
    if (begun && models.models && !models.modelsReady && !models.downloading) models.download();
    // Once per click: a failed download is resumed by "Reprendre", not by this effect.
  }, [begun, models.models === null]);

  if (!permissions) return null;

  const failed = !!models.downloadError && !models.downloading && !models.modelsReady;
  // The connection is lost but the download is still alive: it shows as interrupted, then resumes by itself.
  const interrupted = failed || models.waiting;

  // The download notices follow the user on every screen: leaving the models screen does not stop the download.
  const toasts: ToastData[] = [];
  if (models.waiting) {
    toasts.push({
      key: "download-error",
      tone: "danger",
      title: "Téléchargement interrompu",
      text: "Connexion perdue. Le téléchargement reprendra tout seul dès qu'elle revient. Le reste est conservé.",
    });
  } else if (models.downloading) {
    toasts.push({
      key: "download",
      tone: "progress",
      title: "Téléchargement des modèles",
      text: speed
        ? `${formatProgress(received, total)} · ${formatSpeed(speed)}`
        : formatProgress(received, total),
      progress: total ? received / total : 0,
    });
  } else if (failed) {
    toasts.push({
      key: "download-error",
      tone: "danger",
      title: "Téléchargement interrompu",
      text: models.downloadError ?? undefined,
      action: { label: "Reprendre", run: models.download },
    });
  }
  toasts.push(...notices);

  if (step === "welcome") {
    return (
      <SetupFrame
        left={
          <WelcomeText
            modelsSize={total || null}
            onStart={() => {
              setBegun(true);
              setStep("permissions");
            }}
          />
        }
        right={<WelcomeDemo />}
        placement="center"
        toasts={toasts}
      />
    );
  }

  if (step === "permissions") {
    return (
      <SetupFrame
        current={0}
        filled={[Number(permissions.microphone) + Number(permissions.accessibility), 0, 0]}
        left={<PermissionsText permissions={permissions} onContinue={() => setStep("models")} />}
        right={
          <PermissionsCards
            permissions={permissions}
            onMicrophoneRefused={() =>
              addNotice({
                key: "microphone-refused",
                tone: "warning",
                title: "Micro non autorisé",
                text: `Active ${APP_NAME} dans Réglages Système.`,
                action: {
                  label: "Ouvrir les réglages",
                  run: () => invoke("open_microphone_settings").catch(() => {}),
                },
              })
            }
          />
        }
        toasts={toasts}
      />
    );
  }

  if (step === "mic") {
    return (
      <SetupFrame
        current={2}
        filled={[2, 1, 0]}
        left={
          <MicTestText
            test={micTest}
            models={
              ready
                ? { label: "Modèles prêts", tone: "success" }
                : interrupted
                  ? { label: "Téléchargement interrompu", tone: "danger" }
                  : { label: "Modèles en cours de téléchargement", tone: "neutral" }
            }
            onBack={() => setStep("models")}
            onContinue={() => setStep("shortcut")}
          />
        }
        right={<MicTestCards test={micTest} />}
        toasts={toasts}
      />
    );
  }

  if (step === "shortcut") {
    return (
      <SetupFrame
        current={2}
        filled={[2, 1, 1]}
        placement="shortcut"
        left={
          <ShortcutText
            capturing={shortcutCapture.capturing}
            canContinue={shortcutTest.recognized}
            onBack={shortcutCapture.capturing ? shortcutCapture.cancel : () => setStep("mic")}
            onContinue={() => setStep("demo")}
          />
        }
        right={
          <ShortcutCards
            label={shortcut?.label ?? null}
            down={shortcutTest.down}
            pressed={shortcutTest.pressed}
            recognized={shortcutTest.recognized}
            capturing={shortcutCapture.capturing}
            holdProgress={shortcutCapture.progress}
            error={[shortcutCapture.error, shortcut?.error].filter(Boolean).join(" ") || null}
            onModify={shortcutCapture.start}
            onCancel={shortcutCapture.cancel}
          />
        }
        toasts={toasts}
      />
    );
  }

  if (step === "demo") {
    return (
      <SetupFrame
        current={2}
        filled={[2, 1, 2]}
        sinkToast={!ready}
        left={
          <DemoText locked={!ready} onBack={() => setStep("shortcut")} onContinue={() => setStep("learn")} />
        }
        right={<DemoCards shortcutLabel={shortcut?.label ?? null} locked={!ready} demo={demo} field={demoField} />}
        toasts={toasts}
      />
    );
  }

  if (step === "learn") {
    return (
      <SetupFrame
        current={2}
        filled={[2, 1, 3]}
        placement="shortcut"
        left={
          <ShortcutText
            kind="learn"
            capturing={learnCapture.capturing}
            canContinue={learnTest.recognized}
            onBack={learnCapture.capturing ? learnCapture.cancel : () => setStep("demo")}
            onContinue={() => setStep("learnDemo")}
          />
        }
        right={
          <ShortcutCards
            label={learnShortcut?.label ?? null}
            down={learnTest.down}
            pressed={learnTest.pressed}
            recognized={learnTest.recognized}
            capturing={learnCapture.capturing}
            holdProgress={learnCapture.progress}
            error={[learnCapture.error, learnShortcut?.error].filter(Boolean).join(" ") || null}
            onModify={learnCapture.start}
            onCancel={learnCapture.cancel}
          />
        }
        toasts={toasts}
      />
    );
  }

  if (step === "learnDemo") {
    return (
      <SetupFrame
        current={2}
        filled={[2, 1, 4]}
        sinkToast
        left={<LearnText onBack={() => setStep("learn")} onContinue={() => setStep("ready")} />}
        right={<LearnCards shortcutLabel={learnShortcut?.label ?? null} demo={learnDemo} />}
        toasts={toasts}
      />
    );
  }

  if (step === "ready") {
    return (
      <SetupFrame
        current={3}
        filled={[2, 1, 5]}
        left={<ReadyText canFinish={ready} onBack={() => setStep("learnDemo")} onDone={onDone} />}
        right={
          <ReadyCards
            modelsSize={total || null}
            modelsReady={ready}
            shortcut={shortcut?.label ?? null}
            learnShortcut={learnShortcut?.label ?? null}
          />
        }
        toasts={toasts}
      />
    );
  }

  return (
    <SetupFrame
      current={1}
      filled={[2, models.modelsReady ? 1 : 0, 0]}
      failed={interrupted}
      left={
        <ModelsText
          modelsError={models.modelsError}
          transcription={transcription}
          rewriting={rewriting}
          modelsReady={models.modelsReady}
          onBack={() => setStep("permissions")}
          onContinue={() => setStep("mic")}
        />
      }
      right={
        <ModelsCards
          models={models.models}
          progress={models.progress}
          downloading={models.downloading}
          waiting={models.waiting}
          downloadError={models.downloadError}
          transcription={transcription}
          rewriting={rewriting}
        />
      }
      toasts={toasts}
    />
  );
}

export default Setup;
