import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "preact/hooks";
import type { PreInputPayload } from "../domain";
import { preinputApi } from "../ipc/client";

const FAST_SYNC_INTERVAL_MS = 50;
const FAST_SYNC_DURATION_MS = 1000;

const initialPayload: PreInputPayload = {
  sessionId: 0,
  text: "",
  state: "recording",
  confirmedChars: 0,
  message: "正在聆听",
  seq: 0,
};

export function usePreInputPayload() {
  const [payload, setPayload] = useState<PreInputPayload>(initialPayload);
  const [visible, setVisible] = useState(false);
  const latestSession = useRef(0);
  const latestSeq = useRef(0);
  const closedSession = useRef(0);

  useEffect(() => {
    document.documentElement.classList.add("preinput-root");
    document.body.classList.add("preinput-body");
    let disposed = false;
    let fastSyncTimer: number | undefined;
    let fastSyncStopTimer: number | undefined;

    const acceptPayload = (nextPayload: PreInputPayload) => {
      if (nextPayload.sessionId < latestSession.current) return;
      if (nextPayload.sessionId === closedSession.current) return;
      if (nextPayload.sessionId > latestSession.current) {
        latestSession.current = nextPayload.sessionId;
        latestSeq.current = 0;
      }
      if (nextPayload.seq <= latestSeq.current) return;
      latestSeq.current = nextPayload.seq;
      setPayload(nextPayload);
      setVisible(true);
    };

    const syncPayload = async () => {
      try {
        const nextPayload = await preinputApi.getPayload();
        if (disposed) return;
        if (nextPayload) acceptPayload(nextPayload);
        else setVisible(false);
      } catch {
        // 后端尚未就绪时，悬浮预输入框保持安静。
      }
    };

    const stopFastSync = () => {
      if (fastSyncTimer !== undefined) {
        window.clearInterval(fastSyncTimer);
        fastSyncTimer = undefined;
      }
      if (fastSyncStopTimer !== undefined) {
        window.clearTimeout(fastSyncStopTimer);
        fastSyncStopTimer = undefined;
      }
    };

    const startFastSync = () => {
      stopFastSync();
      void syncPayload();
      fastSyncTimer = window.setInterval(syncPayload, FAST_SYNC_INTERVAL_MS);
      fastSyncStopTimer = window.setTimeout(stopFastSync, FAST_SYNC_DURATION_MS);
    };

    const unlistenShow = listen<PreInputPayload>("preinput_show", (event) => {
      acceptPayload(event.payload);
      startFastSync();
    });
    const unlistenUpdate = listen<PreInputPayload>("preinput_update", (event) => {
      acceptPayload(event.payload);
    });
    const unlistenHide = listen<PreInputPayload>("preinput_hide", (event) => {
      stopFastSync();
      if (event.payload.sessionId >= latestSession.current) {
        latestSession.current = event.payload.sessionId;
        latestSeq.current = event.payload.seq;
        closedSession.current = event.payload.sessionId;
      }
      setVisible(false);
    });
    startFastSync();

    return () => {
      disposed = true;
      stopFastSync();
      document.documentElement.classList.remove("preinput-root");
      document.body.classList.remove("preinput-body");
      unlistenShow.then((dispose) => dispose());
      unlistenUpdate.then((dispose) => dispose());
      unlistenHide.then((dispose) => dispose());
    };
  }, []);

  return { payload, visible };
}
