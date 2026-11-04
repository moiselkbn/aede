import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type ModelInfo = { label: string; file_name: string; size: number; present: boolean };
export type Progress = {
  file_name: string;
  step: "download" | "check";
  downloaded: number;
  total: number;
};
type Finished = { error: string | null };

/** The models on disk and the state of their download (events sent by download.rs). */
export function useModels() {
  const [models, setModels] = useState<ModelInfo[] | null>(null);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const [progress, setProgress] = useState<Record<string, Progress>>({});
  const [downloading, setDownloading] = useState(false);
  const [downloadError, setDownloadError] = useState<string | null>(null);
  // Bytes that really came in during this session. A resumed file reports what it already has on
  // disk with its first event: that jump is not speed, so it is not counted.
  const [transferred, setTransferred] = useState(0);
  const lastSeen = useRef<Record<string, number>>({});
  // The connection is lost: the download waits and tries again by itself.
  const [waiting, setWaiting] = useState(false);

  const refreshModels = () =>
    invoke<ModelInfo[]>("get_models_status")
      .then((list) => {
        setModels(list);
        setModelsError(null);
      })
      .catch((err) => setModelsError(String(err)));

  useEffect(() => {
    refreshModels();
  }, []);

  useEffect(() => {
    let cancelled = false;
    const unlisteners: (() => void)[] = [];
    const subscribe = <T,>(name: string, handler: (payload: T) => void) =>
      listen<T>(name, (event) => handler(event.payload)).then((fn) => {
        if (cancelled) fn();
        else unlisteners.push(fn);
      });
    subscribe<Progress>("download-progress", (p) => {
      setProgress((prev) => ({ ...prev, [p.file_name]: p }));
      const before = lastSeen.current[p.file_name];
      lastSeen.current[p.file_name] = p.downloaded;
      if (before !== undefined && p.downloaded > before) {
        setTransferred((total) => total + p.downloaded - before);
      }
    });
    subscribe<{ waiting: boolean }>("download-waiting", (p) => setWaiting(p.waiting));
    subscribe<Finished>("download-finished", ({ error }) => {
      setDownloading(false);
      setWaiting(false);
      setDownloadError(error);
      refreshModels();
    });
    return () => {
      cancelled = true;
      unlisteners.forEach((fn) => fn());
    };
  }, []);

  async function download() {
    setDownloadError(null);
    setDownloading(true);
    try {
      await invoke("download_models");
    } catch (err) {
      setDownloading(false);
      setDownloadError(String(err));
    }
  }

  const missing = models?.filter((m) => !m.present) ?? [];
  return {
    models,
    modelsError,
    progress,
    transferred,
    downloading,
    waiting,
    downloadError,
    download,
    missing,
    missingSize: missing.reduce((sum, m) => sum + m.size, 0),
    modelsReady: models !== null && missing.length === 0,
  };
}
