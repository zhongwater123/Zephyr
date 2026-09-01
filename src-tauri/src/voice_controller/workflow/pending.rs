use super::super::contract::{
    PendingDeliveryJob, PendingDeliveryOutcome, VoiceInternalEvent, VoiceInternalEventSink,
};
use crate::delivery::DeliveryService;
use crate::history;
use crate::inject::SubmissionState;
use crate::target::DeliveryCertainty;

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
        targets,
        executor,
        delivery_mode,
        services,
        config,
    } = job;
    let text = lease.record().dto.text.clone();
    let target = lease.record().target.clone();
    let current_certainty = lease.record().dto.delivery_certainty;
    let metadata = lease.metadata().clone();
    let delivery = DeliveryService::new(services, targets);
    let text = match delivery.validate_with_intent(&text, &target, true, metadata.intent) {
        Ok(text) => text,
        Err(error) => {
            return PendingDeliveryOutcome::Retained {
                lease,
                code: error.code,
                message: error.message,
                certainty: current_certainty,
            }
        }
    };
    let receipt = match delivery
        .inject_with_intent(
            text.clone(),
            target.clone(),
            executor,
            delivery_mode,
            metadata.intent,
        )
        .await
    {
        Ok(receipt) => receipt,
        Err(error) => {
            return PendingDeliveryOutcome::Retained {
                lease,
                code: error.code,
                message: error.message,
                certainty: current_certainty,
            }
        }
    };
    match receipt.submission {
        SubmissionState::NotSubmitted => {
            return PendingDeliveryOutcome::Retained {
                lease,
                code: "injection_not_submitted",
                message: "未向目标提交任何输入事件".to_string(),
                certainty: current_certainty,
            }
        }
        SubmissionState::Unknown => {
            return PendingDeliveryOutcome::Retained {
                lease,
                code: "delivery_submission_unknown",
                message: "文本可能已经输入，请先检查目标窗口；系统不会自动重试".to_string(),
                certainty: DeliveryCertainty::MayHaveBeenSubmitted,
            }
        }
        SubmissionState::Submitted => {}
    }
    let _ = delivery
        .commit_with_provenance(
            text,
            history::AppContext {
                app_name: Some(target.context().application_key.clone()),
                app_title: target.context().window_title.clone(),
            },
            config,
            metadata.provenance,
        )
        .await;
    PendingDeliveryOutcome::Delivered { lease }
}
