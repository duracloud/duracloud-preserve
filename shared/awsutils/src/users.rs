use aws_sdk_iam::Client;
use aws_sdk_iam::types::{Tag, User};
use futures::{StreamExt, TryStreamExt, stream};

const MAX_USER_CONCURRENCY: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserInfo {
    pub user_name: String,
    pub email: String,
    pub groups: Vec<String>,
}

/// List IAM users with their group memberships and `Email` tag value.
///
/// Users with no group memberships or without an `Email` tag are filtered out of
/// the result — this is intentional and not a partial listing.
pub async fn list_users_with_email_and_groups(
    client: &Client,
) -> Result<Vec<UserInfo>, aws_sdk_iam::Error> {
    let users = list_all_users(client).await?;

    stream::iter(users)
        .map(|user| process_user(client, user))
        .buffered(MAX_USER_CONCURRENCY)
        .try_filter_map(|opt| async move { Ok(opt) })
        .try_collect()
        .await
}

pub async fn list_all_users(client: &Client) -> Result<Vec<User>, aws_sdk_iam::Error> {
    let mut stream = client.list_users().into_paginator().send();
    let mut out = Vec::new();
    while let Some(page) = stream.try_next().await? {
        out.extend(page.users().iter().cloned());
    }
    Ok(out)
}

pub async fn list_group_names_for_user(
    client: &Client,
    user_name: &str,
) -> Result<Vec<String>, aws_sdk_iam::Error> {
    let mut stream = client
        .list_groups_for_user()
        .user_name(user_name)
        .into_paginator()
        .send();
    let mut out = Vec::new();
    while let Some(page) = stream.try_next().await? {
        out.extend(page.groups().iter().map(|g| g.group_name().to_string()));
    }
    out.sort();
    Ok(out)
}

pub async fn get_user_tag(
    client: &Client,
    user_name: &str,
    key: &str,
) -> Result<Option<String>, aws_sdk_iam::Error> {
    let mut stream = client
        .list_user_tags()
        .user_name(user_name)
        .into_paginator()
        .send();
    while let Some(page) = stream.try_next().await? {
        if let Some(value) = find_tag(page.tags(), key) {
            return Ok(Some(value.to_string()));
        }
    }
    Ok(None)
}

fn find_tag<'a>(tags: &'a [Tag], key: &str) -> Option<&'a str> {
    tags.iter().find(|tag| tag.key() == key).map(Tag::value)
}

async fn process_user(client: &Client, user: User) -> Result<Option<UserInfo>, aws_sdk_iam::Error> {
    let user_name = user.user_name();

    let groups = list_group_names_for_user(client, user_name).await?;
    if groups.is_empty() {
        return Ok(None);
    }

    let Some(email) = get_user_tag(client, user_name, "Email").await? else {
        return Ok(None);
    };

    Ok(Some(UserInfo {
        user_name: user_name.to_string(),
        email,
        groups,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_support::{
        mock_sdk_config, mock_sdk_config_with_events, recorded_requests, replay_xml_event,
    };

    fn users_response(users: &[&str], next_marker: Option<&str>) -> String {
        let is_truncated = next_marker.is_some();
        let marker_xml = next_marker
            .map(|m| format!("<Marker>{m}</Marker>"))
            .unwrap_or_default();
        let members: String = users
            .iter()
            .map(|name| {
                format!(
                    "<member><UserName>{name}</UserName>\
                     <UserId>AID{name}</UserId>\
                     <Arn>arn:aws:iam::123456789012:user/{name}</Arn>\
                     <Path>/</Path>\
                     <CreateDate>2024-01-01T00:00:00Z</CreateDate></member>"
                )
            })
            .collect();
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ListUsersResponse xmlns="https://iam.amazonaws.com/doc/2010-05-08/">
  <ListUsersResult>
    <IsTruncated>{is_truncated}</IsTruncated>
    {marker_xml}
    <Users>{members}</Users>
  </ListUsersResult>
  <ResponseMetadata><RequestId>req-1</RequestId></ResponseMetadata>
</ListUsersResponse>"#
        )
    }

    fn groups_response(groups: &[&str], next_marker: Option<&str>) -> String {
        let is_truncated = next_marker.is_some();
        let marker_xml = next_marker
            .map(|m| format!("<Marker>{m}</Marker>"))
            .unwrap_or_default();
        let members: String = groups
            .iter()
            .map(|name| {
                format!(
                    "<member><GroupName>{name}</GroupName>\
                     <GroupId>AGP{name}</GroupId>\
                     <Arn>arn:aws:iam::123456789012:group/{name}</Arn>\
                     <Path>/</Path>\
                     <CreateDate>2024-01-01T00:00:00Z</CreateDate></member>"
                )
            })
            .collect();
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ListGroupsForUserResponse xmlns="https://iam.amazonaws.com/doc/2010-05-08/">
  <ListGroupsForUserResult>
    <IsTruncated>{is_truncated}</IsTruncated>
    {marker_xml}
    <Groups>{members}</Groups>
  </ListGroupsForUserResult>
  <ResponseMetadata><RequestId>req-1</RequestId></ResponseMetadata>
</ListGroupsForUserResponse>"#
        )
    }

    fn tags_response(tags: &[(&str, &str)], next_marker: Option<&str>) -> String {
        let is_truncated = next_marker.is_some();
        let marker_xml = next_marker
            .map(|m| format!("<Marker>{m}</Marker>"))
            .unwrap_or_default();
        let members: String = tags
            .iter()
            .map(|(k, v)| format!("<member><Key>{k}</Key><Value>{v}</Value></member>"))
            .collect();
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ListUserTagsResponse xmlns="https://iam.amazonaws.com/doc/2010-05-08/">
  <ListUserTagsResult>
    <IsTruncated>{is_truncated}</IsTruncated>
    {marker_xml}
    <Tags>{members}</Tags>
  </ListUserTagsResult>
  <ResponseMetadata><RequestId>req-1</RequestId></ResponseMetadata>
</ListUserTagsResponse>"#
        )
    }

    fn make_tag(key: &str, value: &str) -> Tag {
        Tag::builder()
            .key(key)
            .value(value)
            .build()
            .expect("tag should build")
    }

    #[test]
    fn test_find_tag_returns_none_when_missing() {
        let tags = [make_tag("Department", "Eng")];
        assert!(find_tag(&tags, "Email").is_none());
    }

    #[test]
    fn test_find_tag_returns_value_when_present() {
        let tags = [
            make_tag("Department", "Eng"),
            make_tag("Email", "alice@example.com"),
        ];
        assert_eq!(find_tag(&tags, "Email"), Some("alice@example.com"));
    }

    #[tokio::test]
    async fn test_get_user_tag_finds_value_on_second_page() {
        let (sdk_config, _replay) = mock_sdk_config_with_events(vec![
            replay_xml_event(200, tags_response(&[("Department", "Eng")], Some("page2"))),
            replay_xml_event(200, tags_response(&[("Email", "alice@example.com")], None)),
        ]);
        let client = Client::new(&sdk_config);

        let result = get_user_tag(&client, "alice", "Email")
            .await
            .expect("get_user_tag should succeed");

        assert_eq!(result.as_deref(), Some("alice@example.com"));
    }

    #[tokio::test]
    async fn test_get_user_tag_returns_none_when_missing() {
        let (sdk_config, _replay) = mock_sdk_config(replay_xml_event(
            200,
            tags_response(&[("Department", "Eng")], None),
        ));
        let client = Client::new(&sdk_config);

        let result = get_user_tag(&client, "alice", "Email")
            .await
            .expect("get_user_tag should succeed");

        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_get_user_tag_returns_value_when_present() {
        let (sdk_config, _replay) = mock_sdk_config(replay_xml_event(
            200,
            tags_response(&[("Email", "alice@example.com")], None),
        ));
        let client = Client::new(&sdk_config);

        let result = get_user_tag(&client, "alice", "Email")
            .await
            .expect("get_user_tag should succeed");

        assert_eq!(result.as_deref(), Some("alice@example.com"));
    }

    #[tokio::test]
    async fn test_list_all_users_maps_iam_failures_to_error() {
        let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<ErrorResponse xmlns="https://iam.amazonaws.com/doc/2010-05-08/">
  <Error>
    <Type>Sender</Type>
    <Code>AccessDenied</Code>
    <Message>not authorized</Message>
  </Error>
  <RequestId>req-err</RequestId>
</ErrorResponse>"#;
        let (sdk_config, _replay) = mock_sdk_config(replay_xml_event(403, body));
        let client = Client::new(&sdk_config);

        let err = list_all_users(&client)
            .await
            .expect_err("list_all_users should fail");

        assert!(err.to_string().to_lowercase().contains("access"));
    }

    #[tokio::test]
    async fn test_list_all_users_paginates_across_pages() {
        let (sdk_config, _replay) = mock_sdk_config_with_events(vec![
            replay_xml_event(200, users_response(&["alice"], Some("page2"))),
            replay_xml_event(200, users_response(&["bob"], None)),
        ]);
        let client = Client::new(&sdk_config);

        let users = list_all_users(&client)
            .await
            .expect("list_all_users should succeed");

        let names: Vec<&str> = users.iter().map(User::user_name).collect();
        assert_eq!(names, vec!["alice", "bob"]);
    }

    #[tokio::test]
    async fn test_list_all_users_returns_users() {
        let (sdk_config, _replay) = mock_sdk_config(replay_xml_event(
            200,
            users_response(&["alice", "bob"], None),
        ));
        let client = Client::new(&sdk_config);

        let users = list_all_users(&client)
            .await
            .expect("list_all_users should succeed");

        let names: Vec<&str> = users.iter().map(User::user_name).collect();
        assert_eq!(names, vec!["alice", "bob"]);
    }

    #[tokio::test]
    async fn test_list_group_names_for_user_paginates_across_pages() {
        let (sdk_config, _replay) = mock_sdk_config_with_events(vec![
            replay_xml_event(200, groups_response(&["admins"], Some("page2"))),
            replay_xml_event(200, groups_response(&["users"], None)),
        ]);
        let client = Client::new(&sdk_config);

        let groups = list_group_names_for_user(&client, "alice")
            .await
            .expect("list_group_names_for_user should succeed");

        assert_eq!(groups, vec!["admins", "users"]);
    }

    #[tokio::test]
    async fn test_list_group_names_for_user_returns_sorted_names() {
        let (sdk_config, _replay) = mock_sdk_config(replay_xml_event(
            200,
            groups_response(&["zzz", "admins", "mmm"], None),
        ));
        let client = Client::new(&sdk_config);

        let groups = list_group_names_for_user(&client, "alice")
            .await
            .expect("list_group_names_for_user should succeed");

        assert_eq!(groups, vec!["admins", "mmm", "zzz"]);
    }

    #[tokio::test]
    async fn test_list_users_with_email_and_groups_returns_populated_user() {
        let (sdk_config, replay) = mock_sdk_config_with_events(vec![
            replay_xml_event(200, users_response(&["alice"], None)),
            replay_xml_event(200, groups_response(&["admins"], None)),
            replay_xml_event(200, tags_response(&[("Email", "alice@example.com")], None)),
        ]);
        let client = Client::new(&sdk_config);

        let result = list_users_with_email_and_groups(&client)
            .await
            .expect("orchestration should succeed");

        assert_eq!(
            result,
            vec![UserInfo {
                user_name: "alice".to_string(),
                email: "alice@example.com".to_string(),
                groups: vec!["admins".to_string()],
            }]
        );

        let bodies: Vec<String> = recorded_requests(&replay)
            .into_iter()
            .map(|r| String::from_utf8_lossy(&r.body).into_owned())
            .collect();

        assert_eq!(bodies.len(), 3);
        assert!(bodies[0].contains("Action=ListUsers"));
        assert!(
            bodies[1].contains("Action=ListGroupsForUser") && bodies[1].contains("UserName=alice")
        );
        assert!(bodies[2].contains("Action=ListUserTags") && bodies[2].contains("UserName=alice"));
    }

    #[tokio::test]
    async fn test_list_users_with_email_and_groups_skips_users_without_groups_or_email() {
        let (sdk_config, _replay) = mock_sdk_config_with_events(vec![
            replay_xml_event(200, users_response(&["carol", "dave"], None)),
            replay_xml_event(200, groups_response(&[], None)),
            replay_xml_event(200, groups_response(&["users"], None)),
            replay_xml_event(200, tags_response(&[("Department", "Eng")], None)),
        ]);
        let client = Client::new(&sdk_config);

        let result = list_users_with_email_and_groups(&client)
            .await
            .expect("orchestration should succeed");

        assert!(result.is_empty());
    }
}
