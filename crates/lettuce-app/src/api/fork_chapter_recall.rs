//! Fork-only: get and set folded-chapter recall settings.

use lettuce_contracts::{self as dto, ApiError, ApiErrorCode};
use lettuce_memory::ChapterRecallConfig;

use super::ApiContext;
use super::error::{api_error, invalid_field};

fn storage(error: impl std::fmt::Display) -> ApiError {
    api_error(ApiErrorCode::Internal, error.to_string())
}

fn view(config: ChapterRecallConfig) -> dto::ChapterRecallConfigDto {
    let config = config.clamp();
    dto::ChapterRecallConfigDto {
        enabled: config.enabled,
        limit: config.limit,
    }
}

pub async fn chapter_recall_config_get(
    context: &ApiContext,
) -> Result<dto::ChapterRecallConfigDto, ApiError> {
    context
        .blocking(|context| {
            context
                .backend()
                .database()
                .fork_chapter_recall_config()
                .map(view)
                .map_err(storage)
        })
        .await
}

pub async fn chapter_recall_config_set(
    context: &ApiContext,
    request: dto::ChapterRecallConfigDto,
) -> Result<dto::ChapterRecallConfigDto, ApiError> {
    if !(1..=2).contains(&request.limit) {
        return Err(invalid_field("limit", "limit must be 1 or 2"));
    }
    context
        .blocking(move |context| {
            let config = ChapterRecallConfig {
                enabled: request.enabled,
                limit: request.limit,
            }
            .clamp();
            let database = context.backend().database();
            database
                .fork_chapter_recall_config_set(&config)
                .map_err(storage)?;
            database
                .fork_chapter_recall_config()
                .map(view)
                .map_err(storage)
        })
        .await
}

#[cfg(test)]
mod tests {
    use lettuce_memory::chapter_recall_source;

    use super::*;
    use crate::api::tests::{Reply, harness};

    #[tokio::test(flavor = "multi_thread")]
    async fn config_defaults_off_and_rejects_a_bad_limit() {
        let harness = harness(Reply::Text("ok"));
        let view = chapter_recall_config_get(&harness.context)
            .await
            .expect("get");
        assert!(!view.enabled);
        assert_eq!(view.limit, 2);

        let error = chapter_recall_config_set(
            &harness.context,
            dto::ChapterRecallConfigDto {
                enabled: true,
                limit: 3,
            },
        )
        .await
        .expect_err("bad limit");
        assert_eq!(error.code, dto::ApiErrorCode::InvalidInput);

        let saved = chapter_recall_config_set(
            &harness.context,
            dto::ChapterRecallConfigDto {
                enabled: true,
                limit: 1,
            },
        )
        .await
        .expect("set");
        assert!(saved.enabled);
        assert_eq!(saved.limit, 1);
        let loaded = chapter_recall_config_get(&harness.context)
            .await
            .expect("reload");
        assert_eq!(loaded, saved);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn composition_exposes_the_database_source() {
        let harness = harness(Reply::Text("ok"));
        chapter_recall_config_set(
            &harness.context,
            dto::ChapterRecallConfigDto {
                enabled: true,
                limit: 1,
            },
        )
        .await
        .expect("enable");
        let source = chapter_recall_source().expect("source");
        let config = source.config();
        assert!(config.enabled);
        assert_eq!(config.limit, 1);
        assert!(
            source
                .folded_chapters("missing", "missing", "missing")
                .is_empty()
        );
    }
}
