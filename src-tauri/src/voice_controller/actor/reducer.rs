use super::super::resources::SessionMetrics;
use super::runtime::{VoicePhase, VoiceRuntime};
use crate::state::{ReleaseDecision, VoiceStatePayload};
use crate::voice_trigger::{ActivationId, BeginRejection, VoiceActivation};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Effect {
    StartSession { session_id: u64 },
    StopAudio { session_id: u64 },
    CancelSession { session_id: u64 },
    Publish,
}

pub(super) fn validate_begin(
    runtime: &VoiceRuntime,
    pending_full: bool,
) -> Result<(), BeginRejection> {
    if runtime.phase == VoicePhase::ShuttingDown {
        return Err(BeginRejection::ShuttingDown);
    }
    if !runtime.desired_enabled
        || runtime.availability != super::super::contract::VoiceAvailability::Available
    {
        return Err(BeginRejection::Disabled);
    }
    if runtime.current.is_some() || runtime.phase != VoicePhase::Idle {
        return Err(BeginRejection::Busy);
    }
    if pending_full {
        return Err(BeginRejection::PendingFull);
    }
    Ok(())
}

pub(super) fn begin(
    runtime: &mut VoiceRuntime,
    session_id: u64,
    activation: VoiceActivation,
    config_revision: u64,
    pending_full: bool,
) -> Result<Vec<Effect>, BeginRejection> {
    validate_begin(runtime, pending_full)?;
    runtime
        .begin(session_id, activation, config_revision)
        .ok_or(BeginRejection::Busy)?;
    Ok(vec![Effect::StartSession { session_id }, Effect::Publish])
}

pub(super) fn finish(runtime: &mut VoiceRuntime, activation_id: &ActivationId) -> Vec<Effect> {
    if !runtime.owns_activation(activation_id) {
        return Vec::new();
    }
    let session_id = runtime.current_id().expect("owned activation");
    match runtime.phase {
        VoicePhase::Starting => {
            runtime.clear_current(session_id);
            let payload = runtime.machine.complete();
            runtime.set_payload(payload);
            vec![Effect::CancelSession { session_id }, Effect::Publish]
        }
        VoicePhase::Recording => {
            runtime.phase = VoicePhase::Stopping;
            vec![Effect::StopAudio { session_id }, Effect::Publish]
        }
        _ => Vec::new(),
    }
}

pub(super) fn start_succeeded(runtime: &mut VoiceRuntime, session_id: u64) -> Vec<Effect> {
    match runtime.mark_recording(session_id) {
        Some(()) => vec![Effect::Publish],
        None => vec![Effect::CancelSession { session_id }],
    }
}

pub(super) fn cancel(runtime: &mut VoiceRuntime, activation_id: &ActivationId) -> Vec<Effect> {
    if !runtime.owns_activation(activation_id) {
        return Vec::new();
    }
    let session_id = runtime.current_id().expect("owned activation");
    if runtime.phase == VoicePhase::Pasting {
        return vec![Effect::CancelSession { session_id }];
    }
    runtime.clear_current(session_id);
    let payload = runtime.machine.complete();
    runtime.set_payload(payload);
    vec![Effect::CancelSession { session_id }, Effect::Publish]
}

pub(super) fn set_availability(
    runtime: &mut VoiceRuntime,
    desired_enabled: bool,
    revision: u64,
) -> Vec<Effect> {
    let current = runtime.current_id();
    let pasting = runtime.phase == VoicePhase::Pasting;
    let mut effects = Vec::new();
    if !desired_enabled {
        if let Some(session_id) = current {
            if !pasting {
                runtime.clear_current(session_id);
            }
            effects.push(Effect::CancelSession { session_id });
        }
    }
    runtime.set_desired(desired_enabled, revision);
    effects.push(Effect::Publish);
    effects
}

pub(super) fn recording_deadline(runtime: &mut VoiceRuntime, session_id: u64) -> Vec<Effect> {
    if runtime.current_id() != Some(session_id) || runtime.phase != VoicePhase::Recording {
        return Vec::new();
    }
    runtime.phase = VoicePhase::Stopping;
    vec![Effect::StopAudio { session_id }, Effect::Publish]
}

pub(super) fn set_shortcut_health(
    runtime: &mut VoiceRuntime,
    error: Option<String>,
) -> Vec<Effect> {
    runtime.shortcut_registration_error = error;
    vec![Effect::Publish]
}

pub(super) fn reset_after_error(runtime: &mut VoiceRuntime, session_id: u64) -> Vec<Effect> {
    let matching = runtime.current_id() == Some(session_id)
        || (runtime.current.is_none() && runtime.phase == VoicePhase::Error);
    if !matching {
        return Vec::new();
    }
    runtime.clear_current(session_id);
    runtime.phase = VoicePhase::Idle;
    let payload = runtime.machine.complete();
    runtime.set_payload(payload);
    vec![Effect::Publish]
}

pub(super) fn fail_close(runtime: &mut VoiceRuntime, message: impl Into<String>) -> Vec<Effect> {
    let current = runtime.current_id();
    let pasting = runtime.phase == VoicePhase::Pasting;
    runtime.availability = super::super::contract::VoiceAvailability::Faulted;
    let payload = runtime.machine.fail(message);
    runtime.set_payload(payload);
    if !pasting {
        if let Some(session_id) = current {
            runtime.clear_current(session_id);
        }
        runtime.phase = VoicePhase::Error;
    }
    let mut effects = current
        .map(|session_id| vec![Effect::CancelSession { session_id }])
        .unwrap_or_default();
    effects.push(Effect::Publish);
    effects
}

pub(super) fn fault(runtime: &mut VoiceRuntime, message: impl Into<String>) -> Vec<Effect> {
    runtime.availability = super::super::contract::VoiceAvailability::Faulted;
    let payload = runtime.machine.fail(message);
    runtime.set_payload(payload);
    if runtime.current.is_none() {
        runtime.phase = VoicePhase::Error;
    }
    vec![Effect::Publish]
}

pub(super) fn shutdown(runtime: &mut VoiceRuntime) -> Vec<Effect> {
    let current = runtime.current_id();
    runtime.mark_shutting_down();
    if let Some(session_id) = current {
        runtime.clear_current(session_id);
    }
    let mut effects = current
        .map(|session_id| vec![Effect::CancelSession { session_id }])
        .unwrap_or_default();
    effects.push(Effect::Publish);
    effects
}

pub(super) fn audio_stopped(
    runtime: &mut VoiceRuntime,
    session_id: u64,
    duration: Duration,
) -> Option<ReleaseDecision> {
    if runtime.current_id() != Some(session_id) || runtime.phase != VoicePhase::Stopping {
        return None;
    }
    let decision = runtime.machine.activation_finished(duration);
    match &decision {
        ReleaseDecision::Cancelled { payload, .. } => {
            runtime.set_payload(payload.clone());
            runtime.clear_current(session_id);
        }
        ReleaseDecision::Transcribe { payload, .. } => {
            runtime.phase = VoicePhase::Transcribing;
            runtime.set_payload(payload.clone());
        }
    }
    Some(decision)
}

pub(super) fn authorize_injection(
    runtime: &mut VoiceRuntime,
    session_id: u64,
) -> Option<VoiceStatePayload> {
    if runtime.current_id() != Some(session_id)
        || runtime.phase != VoicePhase::Transcribing
        || !runtime.desired_enabled
        || runtime.availability != super::super::contract::VoiceAvailability::Available
    {
        return None;
    }
    runtime.phase = VoicePhase::Pasting;
    let payload = runtime.machine.paste_started();
    Some(runtime.set_payload(payload))
}

pub(super) fn complete(runtime: &mut VoiceRuntime, session_id: u64) -> Option<VoiceStatePayload> {
    if runtime.current_id() != Some(session_id) {
        return None;
    }
    runtime.clear_current(session_id);
    let payload = runtime.machine.complete();
    Some(runtime.set_payload(payload))
}

pub(super) fn override_message(
    runtime: &mut VoiceRuntime,
    mut payload: VoiceStatePayload,
    message: String,
) -> VoiceStatePayload {
    payload.message = message;
    runtime.replace_payload(payload.clone());
    payload
}

pub(super) fn record_metrics(runtime: &mut VoiceRuntime, metrics: SessionMetrics) {
    runtime.last_metrics = Some(metrics);
}

pub(super) fn record_outcome(
    runtime: &mut VoiceRuntime,
    session_id: u64,
    final_state: &str,
    reason: Option<&str>,
) {
    runtime.record_outcome(session_id, final_state, reason);
}

pub(super) fn fail(
    runtime: &mut VoiceRuntime,
    session_id: u64,
    message: impl Into<String>,
) -> Option<VoiceStatePayload> {
    if runtime.current_id() != Some(session_id) {
        return None;
    }
    runtime.clear_current(session_id);
    runtime.phase = VoicePhase::Error;
    let payload = runtime.machine.fail(message);
    Some(runtime.set_payload(payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice_trigger::VoiceActivation;

    #[test]
    fn quick_finish_during_starting_cancels_immediately() {
        let mut runtime = VoiceRuntime::new(true, 7);
        let activation = VoiceActivation::shortcut();
        begin(&mut runtime, 11, activation.clone(), 7, false).unwrap();

        let finish_effects = finish(&mut runtime, &activation.id);
        assert_eq!(runtime.phase, VoicePhase::Idle);
        assert_eq!(runtime.current_id(), None);
        assert_eq!(
            finish_effects,
            vec![Effect::CancelSession { session_id: 11 }, Effect::Publish,]
        );

        let started_effects = start_succeeded(&mut runtime, 11);
        assert_eq!(runtime.phase, VoicePhase::Idle);
        assert_eq!(
            started_effects,
            vec![Effect::CancelSession { session_id: 11 }]
        );
    }

    #[test]
    fn stale_activation_cannot_finish_or_cancel_current_session() {
        let mut runtime = VoiceRuntime::new(true, 1);
        let active = VoiceActivation::shortcut();
        let stale = VoiceActivation::shortcut();
        begin(&mut runtime, 9, active, 1, false).unwrap();

        assert!(finish(&mut runtime, &stale.id).is_empty());
        assert!(cancel(&mut runtime, &stale.id).is_empty());
        assert_eq!(runtime.current_id(), Some(9));
    }

    #[test]
    fn disabled_and_pending_full_have_distinct_rejections() {
        let mut disabled = VoiceRuntime::new(false, 3);
        assert_eq!(
            begin(&mut disabled, 1, VoiceActivation::shortcut(), 3, false).unwrap_err(),
            BeginRejection::Disabled
        );

        let mut enabled = VoiceRuntime::new(true, 3);
        assert_eq!(
            begin(&mut enabled, 1, VoiceActivation::shortcut(), 3, true).unwrap_err(),
            BeginRejection::PendingFull
        );
    }

    #[test]
    fn disabling_cancels_starting_and_rejects_followup_begin() {
        let mut runtime = VoiceRuntime::new(true, 3);
        begin(&mut runtime, 4, VoiceActivation::shortcut(), 3, false).unwrap();

        assert_eq!(
            set_availability(&mut runtime, false, 4),
            vec![Effect::CancelSession { session_id: 4 }, Effect::Publish]
        );
        assert_eq!(runtime.current_id(), None);
        assert_eq!(
            runtime.availability,
            super::super::super::contract::VoiceAvailability::Disabled
        );
        assert_eq!(
            begin(&mut runtime, 5, VoiceActivation::shortcut(), 4, false).unwrap_err(),
            BeginRejection::Disabled
        );
    }

    #[test]
    fn stale_start_result_is_cancelled_after_disable() {
        let mut runtime = VoiceRuntime::new(true, 1);
        begin(&mut runtime, 8, VoiceActivation::shortcut(), 1, false).unwrap();
        set_availability(&mut runtime, false, 2);
        assert_eq!(
            start_succeeded(&mut runtime, 8),
            vec![Effect::CancelSession { session_id: 8 }]
        );
    }

    #[test]
    fn shortcut_health_does_not_change_availability() {
        let mut runtime = VoiceRuntime::new(true, 9);
        set_shortcut_health(&mut runtime, Some("hook failed".to_string()));
        assert_eq!(
            runtime.availability,
            super::super::super::contract::VoiceAvailability::Available
        );
        assert_eq!(runtime.snapshot().payload.message, "hook failed");

        set_shortcut_health(&mut runtime, None);
        assert_ne!(runtime.snapshot().payload.message, "hook failed");
    }

    #[test]
    fn shutting_down_has_a_distinct_begin_rejection() {
        let mut runtime = VoiceRuntime::new(true, 1);
        shutdown(&mut runtime);
        assert_eq!(
            begin(&mut runtime, 2, VoiceActivation::shortcut(), 1, false).unwrap_err(),
            BeginRejection::ShuttingDown
        );
    }
}
