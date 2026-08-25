import { useState } from "preact/hooks";
import type { PendingOutput } from "../../domain";
import { pendingApi } from "../../ipc/client";
import { commandErrorMessage } from "../../security-model";

export function usePendingOutputs(onNotice: (message: string) => void) {
  const [pendingOutputs, setPendingOutputs] = useState<PendingOutput[]>([]);

  async function refreshPendingOutputs() {
    try {
      setPendingOutputs(await pendingApi.list());
    } catch (error) {
      onNotice(commandErrorMessage(error));
    }
  }

  async function deliverPendingOutput(id: string) {
    try {
      await pendingApi.deliver(id);
      await refreshPendingOutputs();
      onNotice("待处理结果已发送到原窗口。 ");
    } catch (error) {
      onNotice(commandErrorMessage(error));
      await refreshPendingOutputs();
    }
  }

  async function copyPendingOutput(id: string) {
    try {
      await pendingApi.copy(id);
      await refreshPendingOutputs();
      onNotice("文本已由你主动复制到剪贴板。 ");
    } catch (error) {
      onNotice(commandErrorMessage(error));
    }
  }

  async function discardPendingOutput(id: string) {
    try {
      await pendingApi.discard(id);
      await refreshPendingOutputs();
    } catch (error) {
      onNotice(commandErrorMessage(error));
    }
  }

  return {
    pendingOutputs,
    refreshPendingOutputs,
    deliverPendingOutput,
    copyPendingOutput,
    discardPendingOutput,
  };
}
