import { useCallback, useEffect, useRef, useState } from "react";
import { load, type Store } from "@tauri-apps/plugin-store";
import type { Settings } from "../types";

const STORE_FILE = "settings.json";
const STORE_KEY = "settings";

export const defaultSettings: Settings = {
  postMode: "confirm",
  rubyMode: "convert",
  postIntervalSec: 3,
  fetchIntervalSec: 1,
  defaultSource: "kakuyomu",
  defaultTarget: "narou",
  managementUrls: { kakuyomu: "", narou: "" },
};

/** アプリのデータフォルダの settings.json に設定を保存する */
export function useSettings() {
  const [settings, setSettings] = useState<Settings>(defaultSettings);
  const [loaded, setLoaded] = useState(false);
  const storeRef = useRef<Store | null>(null);

  useEffect(() => {
    let cancelled = false;
    load(STORE_FILE, { autoSave: true, defaults: {} })
      .then(async (store) => {
        storeRef.current = store;
        const saved = await store.get<Partial<Settings>>(STORE_KEY);
        if (cancelled) return;
        setSettings({
          ...defaultSettings,
          ...saved,
          managementUrls: { ...defaultSettings.managementUrls, ...saved?.managementUrls },
        });
      })
      .catch((error) => console.error("設定の読み込みに失敗しました", error))
      .finally(() => {
        if (!cancelled) setLoaded(true);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const updateSettings = useCallback((patch: Partial<Settings>) => {
    setSettings((current) => {
      const next = { ...current, ...patch };
      storeRef.current?.set(STORE_KEY, next).catch((error) => console.error("設定の保存に失敗しました", error));
      return next;
    });
  }, []);

  return { settings, loaded, updateSettings };
}
