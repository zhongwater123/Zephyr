use super::super::contract::{
    PendingDeliveryJob, PendingDeliveryOutcome, VoiceInternalEvent, VoiceInternalEventSink,
};
use crate::delivery::DeliveryService;
use crate::history;

pub(crate) fn spawn_pending_delivery(job: PendingDeliveryJob, events: VoiceInternalEventSink) {
    tauri::async_runtime::spawn(async move {
        let outcome = deliver_pending(job).await;
        let _ = events
            .send(VoiceInternalEvent::PendingDeliveryFinished(outcome))
            .await;
    });
}

async fn deliver_pending(job: PendingDeliveryJob) -> PendingDeliveryOutcome {
    let PendingDeliveryJob {
        lease,
        injector,
        injection_method,
        services,
        config,
    } = job;
    let text = lease.record().dto.text.clone();
    let target = lease.record().target.clone();
    let metadata = lease.metadata().clone();
    let delivery = DeliveryService::new(services);
    let text = match delivery.validate_with_intent(&text, &target, true, metadata.intent) {
        Ok(text) => text,
        Err(error) => {
            return PendingDeliveryOutcome::Retained {
                lease,
                code: error.code,
                message: error.message,
            }
        }
    };
    if let Err(error) = delivery
        .inject_with_intent(
            text.clone(),
            target.clone(),
            injector,
            injection_method,
            metadata.intent,
        )
        .await
    {
        return PendingDeliveryOutcome::Retained {
            lease,
            code: error.code,
            message: error.message,
        };
    }
    let _ = delivery
        .commit_with_provenance(
            text,
            history::AppContext {
                app_name: Some(target.executable_name),
                app_title: target.window_title,
            },
            config,
            metadata.provenance,
        )
        .await;
    PendingDeliveryOutcome::Delivered { lease }
}
