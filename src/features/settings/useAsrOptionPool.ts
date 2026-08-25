import { useCallback, useState } from "preact/hooks";
import type { AsrOptionPool, ConfigValue } from "../../domain";
import { asrApi } from "../../ipc/client";
import { commandErrorMessage, parseCommandError } from "../../security-model";

export function useAsrOptionPool(onNotice: (message: string) => void) {
  const [pool, setPool] = useState<AsrOptionPool | null>(null);
  const [savingOptions, setSavingOptions] = useState<Record<string, boolean>>({});
  const [errors, setErrors] = useState<Record<string, string>>({});
  const saving = Object.values(savingOptions).some(Boolean);

  const load = useCallback(async () => {
    try {
      setPool(await asrApi.getOptionPool());
    } catch (error) {
      onNotice("识别选项加载失败：" + commandErrorMessage(error));
    }
  }, [onNotice]);

  const setOption = useCallback(
    async (optionId: string, value: ConfigValue) => {
      if (!pool || savingOptions[optionId]) return;
      setSavingOptions((current) => ({ ...current, [optionId]: true }));
      setErrors((current) => ({ ...current, [optionId]: "" }));
      const previousPool = pool;
      try {
        const next = await asrApi.setOption({
          optionId,
          value,
          expectedRevision: pool.revision,
        });
        setPool(next);
      } catch (error) {
        const payload = parseCommandError(error);
        const currentPool = payload?.details?.currentPool;
        if (payload?.code === "config_conflict" && currentPool) {
          setPool(currentPool);
          setErrors((current) => ({ ...current, [optionId]: "设置已变化，已加载最新值。" }));
        } else {
          setPool(previousPool);
          setErrors((current) => ({ ...current, [optionId]: commandErrorMessage(error) }));
          await load();
        }
      } finally {
        setSavingOptions((current) => {
          const next = { ...current };
          delete next[optionId];
          return next;
        });
      }
    },
    [load, pool, savingOptions],
  );

  return { pool, saving, savingOptions, errors, load, setOption };
}
