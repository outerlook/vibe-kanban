use axum::{Router, extract::State, response::Json as ResponseJson, routing::get};
use deployment::Deployment;
use executors::executors::claude::SkillsData;
use utils::response::ApiResponse;

use crate::DeploymentImpl;

pub async fn get_skills(
    State(deployment): State<DeploymentImpl>,
) -> ResponseJson<ApiResponse<SkillsData>> {
    let skills = deployment
        .skills_cache()
        .get_skills()
        .await
        .unwrap_or_else(|| SkillsData {
            slash_commands: vec![],
            skills: vec![],
        });

    ResponseJson(ApiResponse::success(skills))
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new().route("/skills", get(get_skills))
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use executors::executors::claude::SkillInfo;
    use local_deployment::LocalDeployment;
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn test_get_skills_returns_empty_when_no_cache() {
        let deployment = LocalDeployment::new().await.unwrap();
        let app = router().with_state(deployment);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/skills")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let api_response: ApiResponse<SkillsData> = serde_json::from_slice(&body).unwrap();

        assert!(api_response.is_success());
        let data = api_response.into_data().unwrap();
        assert!(data.slash_commands.is_empty());
        assert!(data.skills.is_empty());
    }

    #[tokio::test]
    async fn test_get_skills_returns_cached_data() {
        let deployment = LocalDeployment::new().await.unwrap();

        // Populate the cache with test data
        let test_skills = SkillsData {
            slash_commands: vec!["commit".to_string(), "review".to_string()],
            skills: vec![
                SkillInfo {
                    name: "code-review".to_string(),
                    description: Some("Reviews code for issues".to_string()),
                    namespace: Some("dev".to_string()),
                },
                SkillInfo {
                    name: "test-gen".to_string(),
                    description: None,
                    namespace: None,
                },
            ],
        };
        deployment
            .skills_cache()
            .update_skills(test_skills.clone())
            .await;

        let app = router().with_state(deployment);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/skills")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let api_response: ApiResponse<SkillsData> = serde_json::from_slice(&body).unwrap();

        assert!(api_response.is_success());
        let data = api_response.into_data().unwrap();
        assert_eq!(data.slash_commands.len(), 2);
        assert_eq!(data.slash_commands[0], "commit");
        assert_eq!(data.slash_commands[1], "review");
        assert_eq!(data.skills.len(), 2);
        assert_eq!(data.skills[0].name, "code-review");
        assert_eq!(
            data.skills[0].description,
            Some("Reviews code for issues".to_string())
        );
        assert_eq!(data.skills[0].namespace, Some("dev".to_string()));
        assert_eq!(data.skills[1].name, "test-gen");
        assert!(data.skills[1].description.is_none());
        assert!(data.skills[1].namespace.is_none());
    }
}
