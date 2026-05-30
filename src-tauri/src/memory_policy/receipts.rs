use chrono::Utc;
use uuid::Uuid;

use crate::runtime::contracts::{TaskEvent, TaskEventSource};

use super::types::{
    MemoryPolicyAction, MemoryPolicyActionKind, MemoryPolicyDecision, MemoryPolicyExecutionReceipt,
    MemoryPolicyReasonCode, MemoryPolicyReceiptStatus, MemoryPolicyTarget,
};

pub fn build_receipt(
    decision: &MemoryPolicyDecision,
    action: &MemoryPolicyAction,
    status: MemoryPolicyReceiptStatus,
    reason_code: Option<MemoryPolicyReasonCode>,
    artifact_ref: Option<String>,
    target_ref: Option<String>,
    error: Option<String>,
) -> MemoryPolicyExecutionReceipt {
    let now = Utc::now().to_rfc3339();
    MemoryPolicyExecutionReceipt {
        receipt_id: format!("receipt-{}", Uuid::new_v4()),
        decision_id: decision.decision_id.clone(),
        action_id: action.action_id.clone(),
        source: decision.input.source,
        source_event_id: decision.input.source_event_id.clone(),
        task_id: decision.input.task_id.clone(),
        intent_id: decision.input.intent_id.clone(),
        correlation_id: format!("{}:{}", decision.input.source_event_id, action.action_id),
        knowledge_class: decision.knowledge_class,
        action: action.kind,
        target: action.target,
        status,
        reason_code,
        artifact_ref,
        target_ref,
        idempotency_key: action.idempotency_key.clone(),
        created_at: now.clone(),
        completed_at: if matches!(
            status,
            MemoryPolicyReceiptStatus::Succeeded
                | MemoryPolicyReceiptStatus::Rejected
                | MemoryPolicyReceiptStatus::Failed
        ) {
            Some(now)
        } else {
            None
        },
        error,
    }
}

pub fn receipt_artifact_ref(receipt: &MemoryPolicyExecutionReceipt) -> String {
    receipt
        .artifact_ref
        .clone()
        .unwrap_or_else(|| format!("memory-policy://receipt/{}", receipt.receipt_id))
}

pub fn receipt_to_task_event(receipt: &MemoryPolicyExecutionReceipt) -> TaskEvent {
    let source = TaskEventSource::Memory;
    let artifact_ref = receipt_artifact_ref(receipt);
    match receipt.status {
        MemoryPolicyReceiptStatus::Succeeded
            if receipt.action == MemoryPolicyActionKind::MemoryGraphRead =>
        {
            TaskEvent::MemoryRecall {
                ts: receipt.created_at.clone(),
                source,
                task_id: receipt.task_id.clone(),
                target: receipt.target.as_task_event_target().into(),
                artifact_ref,
            }
        }
        MemoryPolicyReceiptStatus::Succeeded => TaskEvent::MemoryWrite {
            ts: receipt.created_at.clone(),
            source,
            task_id: receipt.task_id.clone(),
            target: receipt.target.as_task_event_target().into(),
            artifact_ref,
        },
        MemoryPolicyReceiptStatus::Rejected | MemoryPolicyReceiptStatus::Deferred => {
            TaskEvent::Signal {
                ts: receipt.created_at.clone(),
                source,
                task_id: receipt.task_id.clone(),
                code: format!("{:?}", receipt.status).to_ascii_lowercase(),
                message: format!(
                    "memory policy {:?} for target {}",
                    receipt.status,
                    receipt.target.as_task_event_target()
                ),
            }
        }
        _ => TaskEvent::Warning {
            ts: receipt.created_at.clone(),
            source,
            task_id: receipt.task_id.clone(),
            code: "memory_policy_non_terminal".into(),
            message: format!("memory policy status {:?}", receipt.status),
        },
    }
}

// [R5] receipt_to_eval_event 已删（eval 模块删除；唯一调用者在 eval/ 内，已 dead）。
