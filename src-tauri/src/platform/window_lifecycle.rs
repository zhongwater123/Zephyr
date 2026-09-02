use tauri::{AppHandle, Manager, Runtime, WebviewWindow, Window, WindowEvent};

const MAIN_WINDOW_LABEL: &str = "main";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RestorePhase {
    Lookup,
    Unminimize,
    Show,
    Focus,
}

impl RestorePhase {
    fn as_str(self) -> &'static str {
        match self {
            Self::Lookup => "lookup",
            Self::Unminimize => "unminimize",
            Self::Show => "show",
            Self::Focus => "focus",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct RestoreFailure {
    phase: RestorePhase,
    error_code: &'static str,
    message: String,
}

impl RestoreFailure {
    fn new(phase: RestorePhase, error_code: &'static str, message: impl Into<String>) -> Self {
        Self {
            phase,
            error_code,
            message: message.into(),
        }
    }
}

trait MainWindowOperations {
    fn is_minimized(&self) -> Result<bool, String>;
    fn unminimize(&self) -> Result<(), String>;
    fn show(&self) -> Result<(), String>;
    fn focus(&self) -> Result<(), String>;
}

trait MainWindowCloseOperations {
    fn hide(&self) -> Result<(), String>;
}

impl<R: Runtime> MainWindowCloseOperations for Window<R> {
    fn hide(&self) -> Result<(), String> {
        Window::hide(self).map_err(|error| error.to_string())
    }
}

impl<R: Runtime> MainWindowOperations for WebviewWindow<R> {
    fn is_minimized(&self) -> Result<bool, String> {
        WebviewWindow::is_minimized(self).map_err(|error| error.to_string())
    }

    fn unminimize(&self) -> Result<(), String> {
        WebviewWindow::unminimize(self).map_err(|error| error.to_string())
    }

    fn show(&self) -> Result<(), String> {
        WebviewWindow::show(self).map_err(|error| error.to_string())
    }

    fn focus(&self) -> Result<(), String> {
        WebviewWindow::set_focus(self).map_err(|error| error.to_string())
    }
}

pub fn handle_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if !cfg!(target_os = "windows") || window.label() != MAIN_WINDOW_LABEL {
        return;
    }

    if let WindowEvent::CloseRequested { api, .. } = event {
        if let Err(error) = prevent_close_and_hide(window, || api.prevent_close()) {
            log::error!(
                target: "window_lifecycle",
                "event=main_window_close phase=hide result=failed errorCode=window_hide_failed message={:?}",
                error
            );
        }
    }
}

fn prevent_close_and_hide<W: MainWindowCloseOperations>(
    window: &W,
    prevent_close: impl FnOnce(),
) -> Result<(), String> {
    prevent_close();
    window.hide()
}

pub fn restore_main_window(app: &AppHandle) {
    let window = app.get_webview_window(MAIN_WINDOW_LABEL);
    let failures = restore_window(window.as_ref());

    for failure in failures {
        log::error!(
            target: "window_lifecycle",
            "event=main_window_restore phase={} result=failed errorCode={} message={:?}",
            failure.phase.as_str(),
            failure.error_code,
            failure.message
        );
    }
}

fn restore_window<W: MainWindowOperations>(window: Option<&W>) -> Vec<RestoreFailure> {
    let Some(window) = window else {
        return vec![RestoreFailure::new(
            RestorePhase::Lookup,
            "main_window_missing",
            "main window is not registered",
        )];
    };
    let mut failures = Vec::new();

    match window.is_minimized() {
        Ok(true) => {
            if let Err(error) = window.unminimize() {
                failures.push(RestoreFailure::new(
                    RestorePhase::Unminimize,
                    "window_unminimize_failed",
                    error,
                ));
            }
        }
        Ok(false) => {}
        Err(error) => failures.push(RestoreFailure::new(
            RestorePhase::Unminimize,
            "window_state_query_failed",
            error,
        )),
    }

    if let Err(error) = window.show() {
        failures.push(RestoreFailure::new(
            RestorePhase::Show,
            "window_show_failed",
            error,
        ));
    }
    if let Err(error) = window.focus() {
        failures.push(RestoreFailure::new(
            RestorePhase::Focus,
            "window_focus_failed",
            error,
        ));
    }

    failures
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    struct FakeWindow {
        minimized: Cell<bool>,
        fail_state_query: Cell<bool>,
        fail_unminimize: Cell<bool>,
        fail_show: Cell<bool>,
        fail_focus: Cell<bool>,
        calls: RefCell<Vec<&'static str>>,
    }

    struct FakeCloseWindow<'a> {
        prevented: &'a Cell<bool>,
        fail_hide: bool,
    }

    impl MainWindowCloseOperations for FakeCloseWindow<'_> {
        fn hide(&self) -> Result<(), String> {
            assert!(self.prevented.get(), "close must be prevented before hide");
            if self.fail_hide {
                Err("hide failed".to_string())
            } else {
                Ok(())
            }
        }
    }

    impl FakeWindow {
        fn minimized() -> Self {
            Self {
                minimized: Cell::new(true),
                ..Self::default()
            }
        }
    }

    impl MainWindowOperations for FakeWindow {
        fn is_minimized(&self) -> Result<bool, String> {
            self.calls.borrow_mut().push("is_minimized");
            if self.fail_state_query.get() {
                Err("state query failed".to_string())
            } else {
                Ok(self.minimized.get())
            }
        }

        fn unminimize(&self) -> Result<(), String> {
            self.calls.borrow_mut().push("unminimize");
            if self.fail_unminimize.get() {
                Err("unminimize failed".to_string())
            } else {
                self.minimized.set(false);
                Ok(())
            }
        }

        fn show(&self) -> Result<(), String> {
            self.calls.borrow_mut().push("show");
            if self.fail_show.get() {
                Err("show failed".to_string())
            } else {
                Ok(())
            }
        }

        fn focus(&self) -> Result<(), String> {
            self.calls.borrow_mut().push("focus");
            if self.fail_focus.get() {
                Err("focus failed".to_string())
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn restores_minimized_window_before_showing_and_focusing() {
        let window = FakeWindow::minimized();

        assert!(restore_window(Some(&window)).is_empty());
        assert_eq!(
            *window.calls.borrow(),
            ["is_minimized", "unminimize", "show", "focus"]
        );
    }

    #[test]
    fn close_request_is_prevented_before_main_window_is_hidden() {
        let prevented = Cell::new(false);
        let window = FakeCloseWindow {
            prevented: &prevented,
            fail_hide: false,
        };

        assert!(prevent_close_and_hide(&window, || prevented.set(true)).is_ok());
        assert!(prevented.get());
    }

    #[test]
    fn close_request_keeps_prevention_when_hiding_fails() {
        let prevented = Cell::new(false);
        let window = FakeCloseWindow {
            prevented: &prevented,
            fail_hide: true,
        };

        assert_eq!(
            prevent_close_and_hide(&window, || prevented.set(true)),
            Err("hide failed".to_string())
        );
        assert!(prevented.get());
    }

    #[test]
    fn restores_hidden_window_without_unminimizing() {
        let window = FakeWindow::default();

        assert!(restore_window(Some(&window)).is_empty());
        assert_eq!(*window.calls.borrow(), ["is_minimized", "show", "focus"]);
    }

    #[test]
    fn reports_missing_main_window_at_lookup_phase() {
        let failures = restore_window(None::<&FakeWindow>);

        assert_eq!(
            failures,
            [RestoreFailure::new(
                RestorePhase::Lookup,
                "main_window_missing",
                "main window is not registered"
            )]
        );
    }

    #[test]
    fn reports_each_failed_restore_phase_and_continues() {
        let window = FakeWindow::minimized();
        window.fail_unminimize.set(true);
        window.fail_show.set(true);
        window.fail_focus.set(true);

        let failures = restore_window(Some(&window));

        assert_eq!(
            failures
                .iter()
                .map(|failure| failure.phase)
                .collect::<Vec<_>>(),
            [
                RestorePhase::Unminimize,
                RestorePhase::Show,
                RestorePhase::Focus
            ]
        );
        assert_eq!(
            *window.calls.borrow(),
            ["is_minimized", "unminimize", "show", "focus"]
        );
    }

    #[test]
    fn repeated_restore_reuses_the_same_window() {
        let window = FakeWindow::minimized();

        for _ in 0..20 {
            assert!(restore_window(Some(&window)).is_empty());
        }

        assert_eq!(
            window
                .calls
                .borrow()
                .iter()
                .filter(|call| **call == "show")
                .count(),
            20
        );
        assert_eq!(
            window
                .calls
                .borrow()
                .iter()
                .filter(|call| **call == "focus")
                .count(),
            20
        );
    }
}
