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
    let delivery = DeliveryService::new(services);
    if let Err(error) = delivery.validate(&text, &target, true) {
        return PendingDeliveryOutcome::Retained {
            lease,
            code: error.code,
            message: error.message,
        };
    }
    if let Err(error) = delivery
        .inject(text.clone(), injector, injection_method)
        .await
    {
        return PendingDeliveryOutcome::Retained {
            lease,
            code: error.code,
            message: error.message,
        };
    }
    let _ = delivery
        .commit(
            text,
            history::AppContext {
                app_name: Some(target.executable_name),
                app_title: target.window_title,
            },
            config,
        )
        .await;
    PendingDeliveryOutcome::Delivered { lease }
}
