use chrono::Utc;
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use odo_entity::auth::usr;
use odo_entity::notification::{delivery, email_group_member, event};
use rand::Rng;
use sea_orm::prelude::*;
use sea_orm::{Condition, DatabaseConnection, DbBackend, QueryOrder, QuerySelect, Set, Statement};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

use crate::SmtpConfig;

pub async fn run(
    db: DatabaseConnection,
    smtp: Arc<SmtpConfig>,
    worker_id: String,
    poll_interval: Duration,
    batch_size: u64,
    lease_duration_secs: i64,
) {
    info!(
        worker_id = worker_id,
        poll_secs = poll_interval.as_secs(),
        batch_size = batch_size,
        "Email processor starting"
    );

    loop {
        match poll_and_process(&db, &smtp, &worker_id, batch_size, lease_duration_secs).await {
            Ok(count) => {
                if count > 0 {
                    info!(processed = count, "Processed deliveries");
                    continue;
                }
            }
            Err(e) => {
                error!(error = %e, "Processor error");
            }
        }

        tokio::time::sleep(poll_interval).await;
    }
}

async fn poll_and_process(
    db: &DatabaseConnection,
    smtp: &SmtpConfig,
    worker_id: &str,
    batch_size: u64,
    lease_duration_secs: i64,
) -> Result<usize, sea_orm::DbErr> {
    let now = Utc::now();

    let eligible = Condition::any()
        .add(
            Condition::all()
                .add(delivery::Column::Status.eq("pending"))
                .add(
                    Condition::any()
                        .add(delivery::Column::NextRetryAt.is_null())
                        .add(delivery::Column::NextRetryAt.lte(now)),
                ),
        )
        .add(
            Condition::all()
                .add(delivery::Column::Status.eq("processing"))
                .add(delivery::Column::ProcessingExpiresAt.lt(now)),
        );

    let pending = delivery::Entity::find()
        .filter(
            Condition::all()
                .add(delivery::Column::Channel.eq("email"))
                .add(eligible)
                .add(
                    Condition::any()
                        .add(delivery::Column::ScheduledFor.is_null())
                        .add(delivery::Column::ScheduledFor.lte(now)),
                ),
        )
        .order_by_asc(delivery::Column::CreatedAt)
        .limit(batch_size)
        .all(db)
        .await?;

    let mut count = 0;
    for dlv in &pending {
        let delivery_id = dlv.id;
        if delivery_id == 0 {
            continue;
        }

        if let Err(e) =
            claim_and_process(db, smtp, worker_id, delivery_id, lease_duration_secs).await
        {
            error!(delivery_id = delivery_id, error = %e, "Failed to process delivery");
        } else {
            count += 1;
        }
    }

    Ok(count)
}

async fn claim_and_process(
    db: &DatabaseConnection,
    smtp: &SmtpConfig,
    worker_id: &str,
    delivery_id: i64,
    lease_duration_secs: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    let now = Utc::now();
    let lease_expires = now + chrono::Duration::seconds(lease_duration_secs);

    // Raw SQL required: SeaORM's update_many() does not support RETURNING,
    // and the atomic claim requires UPDATE ... WHERE (compound status check)
    // ... RETURNING id to detect whether this worker won the race.
    let claimed = db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"
            UPDATE notification.delivery SET
                status = 'processing',
                processing_started_at = $1,
                processing_expires_at = $2,
                processing_owner = $3,
                updated_at = $1
            WHERE id = $4 AND (
                (status = 'pending' AND (next_retry_at IS NULL OR next_retry_at <= $1))
                OR (status = 'processing' AND processing_expires_at < $1)
            )
            AND (scheduled_for IS NULL OR scheduled_for <= $1)
            RETURNING id
            "#,
            [
                now.into(),
                lease_expires.into(),
                worker_id.into(),
                delivery_id.into(),
            ],
        ))
        .await?;

    if claimed.is_none() {
        return Ok(());
    }

    let dlv = delivery::Entity::find_by_id(delivery_id)
        .one(db)
        .await?
        .ok_or("Delivery not found after claim")?;

    let evt = event::Entity::find_by_id(dlv.event_id)
        .one(db)
        .await?
        .ok_or("Event not found")?;

    let result = process_email(db, smtp, &dlv, &evt).await;

    finalize_delivery(db, &dlv, result).await?;

    Ok(())
}

struct ProcessingResult {
    success: bool,
    error_message: Option<String>,
    metadata: Option<serde_json::Value>,
}

async fn process_email(
    db: &DatabaseConnection,
    smtp: &SmtpConfig,
    dlv: &delivery::Model,
    _evt: &event::Model,
) -> ProcessingResult {
    let recipients = match resolve_recipients(db, dlv).await {
        Ok(r) if r.is_empty() => {
            return ProcessingResult {
                success: false,
                error_message: Some("No recipients resolved".to_string()),
                metadata: None,
            };
        }
        Ok(r) => r,
        Err(e) => {
            return ProcessingResult {
                success: false,
                error_message: Some(format!("Failed to resolve recipients: {e}")),
                metadata: None,
            };
        }
    };

    let subject = &dlv.title_rendered;
    let body_html = dlv.body_rendered.as_deref().unwrap_or("");

    match send_email(smtp, &recipients, subject, body_html).await {
        Ok(metadata) => ProcessingResult {
            success: true,
            error_message: None,
            metadata: Some(metadata),
        },
        Err(e) => ProcessingResult {
            success: false,
            error_message: Some(e.to_string()),
            metadata: None,
        },
    }
}

async fn resolve_recipients(
    db: &DatabaseConnection,
    dlv: &delivery::Model,
) -> Result<Vec<String>, sea_orm::DbErr> {
    if let Some(user_id) = dlv.recipient_user {
        let user = usr::Entity::find_by_id(user_id).one(db).await?;
        match user {
            Some(u) => Ok(vec![u.email]),
            None => Ok(vec![]),
        }
    } else if let Some(group_id) = dlv.recipient_email_group {
        let members = email_group_member::Entity::find()
            .filter(email_group_member::Column::EmailGroup.eq(group_id))
            .filter(email_group_member::Column::IsActive.eq(true))
            .all(db)
            .await?;
        Ok(members.into_iter().map(|m| m.email).collect())
    } else {
        Ok(vec![])
    }
}

async fn send_email(
    smtp: &SmtpConfig,
    recipients: &[String],
    subject: &str,
    body_html: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let from: Mailbox = format!("{} <{}>", smtp.from_name, smtp.from_email).parse()?;

    let transport = build_smtp_transport(smtp)?;

    let mut successful = Vec::new();
    let mut failed = Vec::new();

    for recipient in recipients {
        let to: Mailbox = match recipient.parse() {
            Ok(m) => m,
            Err(e) => {
                warn!(email = %recipient, error = %e, "Invalid recipient email");
                failed.push(recipient.clone());
                continue;
            }
        };

        let message = Message::builder()
            .from(from.clone())
            .to(to)
            .subject(subject)
            .header(lettre::message::header::ContentType::TEXT_HTML)
            .body(body_html.to_string())?;

        match transport.send(message).await {
            Ok(_resp) => {
                successful.push(recipient.clone());
            }
            Err(e) => {
                warn!(email = %recipient, error = %e, "Failed to send email");
                failed.push(recipient.clone());
            }
        }
    }

    if successful.is_empty() {
        return Err("All recipients failed".into());
    }

    Ok(serde_json::json!({
        "successful_emails": successful,
        "failed_emails": failed,
        "total_recipients": recipients.len(),
    }))
}

fn build_smtp_transport(
    smtp: &SmtpConfig,
) -> Result<AsyncSmtpTransport<Tokio1Executor>, lettre::transport::smtp::Error> {
    let builder = if smtp.use_tls || smtp.use_starttls {
        // relay()/starttls_relay() build TlsParameters with full cert
        // validation and offer no way to relax it, so when the operator
        // has opted into accepting invalid certs we construct the TLS
        // parameters by hand instead.
        let tls_parameters = if smtp.dangerous_accept_invalid_certs {
            TlsParameters::builder(smtp.host.clone())
                .dangerous_accept_invalid_certs(true)
                .dangerous_accept_invalid_hostnames(true)
                .build()?
        } else {
            TlsParameters::new(smtp.host.clone())?
        };

        // use_tls -> implicit TLS (Wrapper); use_starttls -> STARTTLS
        // required. Matches what relay()/starttls_relay() would select;
        // we only override the cert validation, not the TLS enforcement.
        let tls = if smtp.use_tls {
            Tls::Wrapper(tls_parameters)
        } else {
            Tls::Required(tls_parameters)
        };

        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&smtp.host).tls(tls)
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&smtp.host)
    };

    let mut builder = builder.port(smtp.port);

    if !smtp.username.is_empty() && !smtp.password.is_empty() {
        builder = builder.credentials(Credentials::new(
            smtp.username.clone(),
            smtp.password.clone(),
        ));
    }

    Ok(builder.build())
}

async fn finalize_delivery(
    db: &DatabaseConnection,
    dlv: &delivery::Model,
    result: ProcessingResult,
) -> Result<(), sea_orm::DbErr> {
    let now = Utc::now();

    let template = dlv.template_code.as_deref().unwrap_or("-");

    if result.success {
        let mut active: delivery::ActiveModel = dlv.clone().into();
        active.status = Set("delivered".to_string());
        active.processed_at = Set(Some(now.into()));
        active.processing_started_at = Set(None);
        active.processing_expires_at = Set(None);
        active.processing_owner = Set(None);
        active.updated_at = Set(now.into());
        if let Some(meta) = result.metadata {
            active.channel_metadata = Set(Some(meta));
        }
        active.update(db).await?;

        info!(
            delivery_id = dlv.id,
            event_id = dlv.event_id,
            channel = %dlv.channel,
            template = template,
            recipient_user = dlv.recipient_user,
            recipient_email_group = dlv.recipient_email_group,
            "Delivery sent"
        );
    } else {
        let retry_count = dlv.retry_count + 1;
        let error_msg = result
            .error_message
            .unwrap_or_else(|| "unknown error".to_string());

        if retry_count >= dlv.max_retries {
            let mut active: delivery::ActiveModel = dlv.clone().into();
            active.status = Set("failed".to_string());
            active.error_code = Set(Some("MAX_RETRIES".to_string()));
            active.error_message = Set(Some(error_msg.clone()));
            active.processed_at = Set(Some(now.into()));
            active.processing_started_at = Set(None);
            active.processing_expires_at = Set(None);
            active.processing_owner = Set(None);
            active.retry_count = Set(retry_count);
            active.updated_at = Set(now.into());
            active.update(db).await?;

            error!(
                delivery_id = dlv.id,
                event_id = dlv.event_id,
                channel = %dlv.channel,
                template = template,
                recipient_user = dlv.recipient_user,
                recipient_email_group = dlv.recipient_email_group,
                retries = retry_count,
                error = %error_msg,
                "Delivery failed permanently"
            );
        } else {
            let base_delay = (1i64 << retry_count).min(3600);
            let jitter = rand::rng().random_range(0..=base_delay / 2);
            let next_retry = now + chrono::Duration::seconds(base_delay + jitter);

            let mut active: delivery::ActiveModel = dlv.clone().into();
            active.status = Set("pending".to_string());
            active.retry_count = Set(retry_count);
            active.next_retry_at = Set(Some(next_retry.into()));
            active.processing_started_at = Set(None);
            active.processing_expires_at = Set(None);
            active.processing_owner = Set(None);
            active.error_message = Set(Some(error_msg.clone()));
            active.updated_at = Set(now.into());
            active.update(db).await?;

            warn!(
                delivery_id = dlv.id,
                event_id = dlv.event_id,
                channel = %dlv.channel,
                template = template,
                recipient_user = dlv.recipient_user,
                recipient_email_group = dlv.recipient_email_group,
                retry = retry_count,
                max_retries = dlv.max_retries,
                next_retry = %next_retry.to_rfc3339(),
                error = %error_msg,
                "Delivery failed, will retry"
            );
        }
    }

    Ok(())
}
