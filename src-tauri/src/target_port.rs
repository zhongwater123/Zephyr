use std::any::Any;
use std::fmt;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetContext {
    pub application_key: String,
    pub window_title: Option<String>,
    pub process_id: u32,
    pub multiline_may_execute: bool,
}

trait OpaqueTargetPayload: Any + Send + Sync + fmt::Debug {
    fn as_any(&self) -> &dyn Any;
}

impl<T> OpaqueTargetPayload for T
where
    T: Any + Send + Sync + fmt::Debug,
{
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone)]
pub struct CapturedTarget {
    context: TargetContext,
    payload: Arc<dyn OpaqueTargetPayload>,
}

impl CapturedTarget {
    pub(crate) fn new<T>(context: TargetContext, payload: T) -> Self
    where
        T: Any + Send + Sync + fmt::Debug,
    {
        Self {
            context,
            payload: Arc::new(payload),
        }
    }

    pub(crate) fn payload_as<T: Any>(&self) -> Option<&T> {
        self.payload.as_ref().as_any().downcast_ref()
    }

    pub fn context(&self) -> &TargetContext {
        &self.context
    }
}

impl fmt::Debug for CapturedTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CapturedTarget")
            .field("context", &self.context)
            .field("payload", &"<opaque>")
            .finish()
    }
}

pub trait TargetPort: Send + Sync {
    fn capture(&self) -> Result<CapturedTarget, String>;
    fn exists(&self, target: &CapturedTarget) -> Result<(), String>;
    fn validate_foreground(&self, target: &CapturedTarget) -> Result<(), String>;
    fn activate(&self, target: &CapturedTarget) -> Result<(), String>;
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) struct FakeTargetPayload(pub(crate) u64);

    #[derive(Debug)]
    pub(crate) struct FakeTargetPort {
        calls: Mutex<Vec<&'static str>>,
        capture_error: Option<String>,
        exists_error: Option<String>,
        validation_error: Option<String>,
    }

    impl FakeTargetPort {
        pub(crate) fn available() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                capture_error: None,
                exists_error: None,
                validation_error: None,
            }
        }

        pub(crate) fn failing_capture(message: impl Into<String>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                capture_error: Some(message.into()),
                exists_error: None,
                validation_error: None,
            }
        }

        pub(crate) fn failing_exists(message: impl Into<String>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                capture_error: None,
                exists_error: Some(message.into()),
                validation_error: None,
            }
        }

        pub(crate) fn failing_validation(message: impl Into<String>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                capture_error: None,
                exists_error: None,
                validation_error: Some(message.into()),
            }
        }

        pub(crate) fn calls(&self) -> Vec<&'static str> {
            self.calls.lock().unwrap().clone()
        }
    }

    pub(crate) fn fake_target(application_key: &str) -> CapturedTarget {
        fake_target_with_multiline(application_key, false)
    }

    pub(crate) fn fake_target_with_multiline(
        application_key: &str,
        multiline_may_execute: bool,
    ) -> CapturedTarget {
        CapturedTarget::new(
            TargetContext {
                application_key: application_key.to_string(),
                window_title: Some("Target".to_string()),
                process_id: 42,
                multiline_may_execute,
            },
            FakeTargetPayload(7),
        )
    }

    impl TargetPort for FakeTargetPort {
        fn capture(&self) -> Result<CapturedTarget, String> {
            self.calls.lock().unwrap().push("capture");
            match &self.capture_error {
                Some(message) => Err(message.clone()),
                None => Ok(fake_target("notepad.exe")),
            }
        }

        fn exists(&self, _target: &CapturedTarget) -> Result<(), String> {
            self.calls.lock().unwrap().push("exists");
            match &self.exists_error {
                Some(message) => Err(message.clone()),
                None => Ok(()),
            }
        }

        fn validate_foreground(&self, _target: &CapturedTarget) -> Result<(), String> {
            self.calls.lock().unwrap().push("validate_foreground");
            match &self.validation_error {
                Some(message) => Err(message.clone()),
                None => Ok(()),
            }
        }

        fn activate(&self, _target: &CapturedTarget) -> Result<(), String> {
            self.calls.lock().unwrap().push("activate");
            Ok(())
        }
    }

    #[test]
    fn captured_target_keeps_payload_opaque_but_cloneable() {
        let target = fake_target("notepad.exe");
        let cloned = target.clone();
        assert_eq!(cloned.context().application_key, "notepad.exe");
        assert_eq!(
            cloned.payload_as::<FakeTargetPayload>(),
            Some(&FakeTargetPayload(7))
        );
        assert!(!format!("{cloned:?}").contains("FakeTargetPayload"));
    }
}
