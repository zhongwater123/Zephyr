use super::coordinator::{EditCoordinator, HOOK_INTERRUPTED, TRACE_TARGET};

impl EditCoordinator {
    pub(super) fn initialize_runtime(&self) {
        let config = self.config.snapshot();
        let initial_error = self
            .state
            .lock()
            .ok()
            .and_then(|state| state.runtime_error.clone());
        if let Ok(engine) = self.engine_handle() {
            if let Err(error) = engine.set_binding(config.shortcut_binding.as_ref()) {
                self.set_runtime_error(Some(error));
            } else if let Err(error) = engine.set_enabled(
                config.enabled && config.shortcut_binding.is_some() && initial_error.is_none(),
            ) {
                self.set_runtime_error(Some(error));
            }
        }
        self.publish_current_runtime_error();
    }

    pub(super) fn set_enabled(&self, _enabled: bool) -> Result<(), String> {
        let _gate = self
            .operation_gate
            .lock()
            .map_err(|error| error.to_string())?;
        self.interrupt_active_edit_locked("enable_changed", "启用状态变化中断了快捷键录入。");
        let current = self.config.snapshot();
        match self.restore_authoritative_runtime(current.enabled) {
            Ok(_) => {
                self.set_runtime_error(None);
                Ok(())
            }
            Err(message) => {
                self.set_runtime_error(Some(message.clone()));
                Err(message)
            }
        }
    }

    pub(super) fn resume(&self) {
        let Ok(_gate) = self.operation_gate.lock() else {
            self.set_runtime_error(Some("快捷键操作门闩已损坏。".into()));
            return;
        };
        self.interrupt_active_edit_locked(
            "system_resume",
            "系统恢复中断了快捷键录入，请重新设置。",
        );
        match self.restore_authoritative_runtime(true) {
            Ok(_) => self.set_runtime_error(None),
            Err(message) => self.set_runtime_error(Some(message)),
        }
    }

    pub(super) fn shutdown(&self) {
        let engine = {
            let Ok(_gate) = self.operation_gate.lock() else {
                return;
            };
            if let Ok(mut state) = self.state.lock() {
                state.edit = None;
            }
            self.engine.lock().ok().and_then(|mut engine| engine.take())
        };
        if let Some(engine) = engine {
            engine.shutdown();
        }
    }

    pub(super) fn handle_hook_interrupted(&self) {
        let Ok(_gate) = self.operation_gate.lock() else {
            self.set_runtime_error(Some("快捷键操作门闩已损坏。".into()));
            return;
        };
        if self.engine_handle().is_ok_and(|engine| engine.is_healthy()) {
            log::debug!(
                target: TRACE_TARGET,
                "event=hook_interruption_ignored reason=already_recovered"
            );
            return;
        }

        let message = "键盘 Hook 工作线程已退出；旧快捷键当前不可用，请重新设置或重新启用。";
        let transaction = self
            .state
            .lock()
            .ok()
            .and_then(|mut state| state.edit.take());
        self.set_runtime_error(Some(message.to_string()));
        log::error!(
            target: TRACE_TARGET,
            "event=hook_interruption_confirmed phase=runtime result=failed errorCode={} message={:?}",
            HOOK_INTERRUPTED,
            message
        );
        if let Some(transaction) = transaction {
            let outcome = self.outcome_for(
                &self.config.snapshot(),
                false,
                transaction.edit_id,
                transaction.trace_id,
                false,
                Some(HOOK_INTERRUPTED),
                message,
            );
            self.log_engine(
                "edit_interrupted",
                &outcome.trace_id,
                outcome.edit_id,
                transaction.expected_revision,
                outcome.config_revision,
                "interrupt",
                transaction.started_at.elapsed().as_millis(),
                "failed",
                HOOK_INTERRUPTED,
                "failed",
                &outcome.active_label,
            );
            self.observer.emit_interrupted(outcome);
        }
    }
}
