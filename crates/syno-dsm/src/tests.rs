use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::Arc;

use serde_json::json;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

fn api_list() -> Value {
    json!({ "success": true, "data": {
        "SYNO.API.Auth": { "path": "entry.cgi", "minVersion": 1, "maxVersion": 7 },
        "SYNO.Core.Network": { "path": "entry.cgi", "minVersion": 1, "maxVersion": 2 },
        "SYNO.Core.System": { "path": "entry.cgi", "minVersion": 1, "maxVersion": 3 },
        "SYNO.Core.System.Utilization": { "path": "entry.cgi", "minVersion": 1, "maxVersion": 1 },
        "SYNO.Storage.CGI.Storage": { "path": "entry.cgi", "minVersion": 1, "maxVersion": 1 }
    }})
}

async fn mock_nas() -> (MockServer, Client) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/webapi/query.cgi"))
        .respond_with(ResponseTemplate::new(200).set_body_json(api_list()))
        .mount(&server)
        .await;
    let client = Client::with_base(Url::parse(&format!("{}/", server.uri())).unwrap());
    (server, client)
}

fn credentials() -> Credentials {
    Credentials {
        account: "admin".into(),
        password: "secret".into(),
        device_name: "Syno Companion test".into(),
        ..Credentials::default()
    }
}

#[tokio::test]
async fn otp_required_is_reported() {
    let (server, client) = mock_nas().await;
    Mock::given(path("/webapi/entry.cgi"))
        .and(body_string_contains("method=login"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "success": false, "error": { "code": 403 } })),
        )
        .mount(&server)
        .await;

    let error = client.login(&credentials()).await.unwrap_err();
    assert_eq!(error, Error::Auth(AuthError::OtpRequired));
    assert_eq!(error.key(), "otp-required");
}

#[tokio::test]
async fn otp_with_remember_device_returns_token() {
    let (server, client) = mock_nas().await;
    Mock::given(path("/webapi/entry.cgi"))
        .and(body_string_contains("method=login"))
        .and(body_string_contains("version=6"))
        .and(body_string_contains("otp_code=123456"))
        .and(body_string_contains("enable_device_token=yes"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({ "success": true, "data": { "sid": "session-1", "did": "device-1" } }),
        ))
        .mount(&server)
        .await;

    let session = client
        .login(&Credentials {
            otp: Some("123456".into()),
            remember_device: true,
            ..credentials()
        })
        .await
        .unwrap();
    assert_eq!(session.sid, "session-1");
    assert_eq!(session.device_token.as_deref(), Some("device-1"));
}

#[tokio::test]
async fn wrong_password_and_expired_session_are_told_apart() {
    let (server, client) = mock_nas().await;
    Mock::given(path("/webapi/entry.cgi"))
        .and(body_string_contains("password-wrong"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "success": false, "error": { "code": 400 } })),
        )
        .mount(&server)
        .await;
    Mock::given(path("/webapi/entry.cgi"))
        .and(body_string_contains("method=login"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "success": true, "data": { "sid": "s" } })),
        )
        .mount(&server)
        .await;
    Mock::given(path("/webapi/entry.cgi"))
        .and(body_string_contains("api=SYNO.Core.System&"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "success": false, "error": { "code": 119 } })),
        )
        .mount(&server)
        .await;

    let wrong = Credentials {
        password: "password-wrong".into(),
        ..credentials()
    };
    assert_eq!(
        client.login(&wrong).await.unwrap_err(),
        Error::Auth(AuthError::WrongCredentials)
    );

    let session = client.login(&credentials()).await.unwrap();
    assert_eq!(
        client.overview(&session).await.unwrap_err(),
        Error::SessionExpired
    );
}

/// A one-request-per-connection HTTPS server with a self-signed certificate that
/// answers like DSM: the API list on `query.cgi`, a session on everything else.
fn self_signed_nas() -> (u16, String) {
    let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
    let certificate = certified.cert.der().clone();
    let print = tls::fingerprint(&certificate);
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(certified.signing_key.serialize_der().into());
    let config = Arc::new(
        rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![certificate], key)
        .unwrap(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let connection = rustls::ServerConnection::new(config.clone()).unwrap();
            let mut tls = rustls::StreamOwned::new(connection, stream);
            let mut reader = BufReader::new(&mut tls);
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).is_err() {
                continue; // the client refused the certificate
            }
            let mut length = 0;
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                if header.trim().is_empty() {
                    break;
                }
                if let Some(value) = header.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let answer = if request_line.contains("query.cgi") {
                api_list()
            } else {
                json!({ "success": true, "data": { "sid": "tls-session" } })
            }
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}",
                answer.len()
            );
            tls.write_all(response.as_bytes()).unwrap();
            tls.conn.send_close_notify();
            let _ = tls.flush();
        }
    });
    (port, print)
}

#[tokio::test]
async fn self_signed_certificate_needs_trust_then_pin_works() {
    let (port, print) = self_signed_nas();

    let unknown = Client::new("127.0.0.1", port, None).unwrap();
    assert_eq!(
        unknown.login(&credentials()).await.unwrap_err(),
        Error::UntrustedCertificate {
            fingerprint: print.clone()
        }
    );

    let pinned = Client::new("127.0.0.1", port, Some(&print)).unwrap();
    let session = pinned.login(&credentials()).await.unwrap();
    assert_eq!(session.sid, "tls-session");

    let other = "00:".repeat(31) + "00";
    let changed = Client::new("127.0.0.1", port, Some(&other)).unwrap();
    assert_eq!(
        changed.login(&credentials()).await.unwrap_err(),
        Error::CertificateChanged { fingerprint: print }
    );
}

#[tokio::test]
async fn setup_creates_the_owner_before_locking_the_builtin_admin() {
    let (server, client) = mock_nas().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).insert_header("set-cookie", "id=setup; path=/"))
        .mount(&server)
        .await;
    let all_ok = |count: usize| {
        let result: Vec<Value> = (0..count).map(|_| json!({ "success": true })).collect();
        ResponseTemplate::new(200).set_body_json(
            json!({ "success": true, "data": { "has_fail": false, "result": result } }),
        )
    };
    Mock::given(path("/webapi/entry.cgi/SYNO.Entry.Request"))
        .and(body_string_contains("SYNO.Core.Group.Member"))
        .respond_with(all_ok(7))
        .mount(&server)
        .await;
    Mock::given(path("/webapi/entry.cgi/SYNO.Entry.Request"))
        .and(body_string_contains("hide_welcome"))
        .respond_with(all_ok(5))
        .mount(&server)
        .await;
    Mock::given(path("/webapi/entry.cgi"))
        .and(body_string_contains("method=login"))
        .and(body_string_contains("account=owner"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "success": true, "data": { "sid": "owner-session" } })),
        )
        .mount(&server)
        .await;

    let done = client
        .complete_setup(&SetupRequest {
            device_name: "nas-test".into(),
            account: "owner".into(),
            password: "Secret-1234".into(),
            time_zone: "Amsterdam".into(),
            updates: UpdatePolicy::Important,
            device_label: "Syno Companion test".into(),
        })
        .await
        .unwrap();
    assert_eq!(done.session.sid, "owner-session");
    assert!(done.skipped.is_empty());

    let requests = server.received_requests().await.unwrap();
    let setup = requests
        .iter()
        .find(|request| String::from_utf8_lossy(&request.body).contains("SYNO.Core.Group.Member"))
        .expect("setup request");
    let form: Vec<(String, String)> = url::form_urlencoded::parse(&setup.body)
        .into_owned()
        .collect();
    let field = |name: &str| {
        form.iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .unwrap()
    };
    assert_eq!(field("stop_when_error"), "true");
    assert_eq!(field("mode"), "\"sequential\"");
    let calls: Vec<Value> = serde_json::from_str(&field("compound")).unwrap();
    let position = |matches: &dyn Fn(&Value) -> bool, what: &str| {
        calls
            .iter()
            .position(matches)
            .unwrap_or_else(|| panic!("{what} missing"))
    };
    let created = position(
        &|c| c["api"] == "SYNO.Core.User" && c["method"] == "create",
        "create",
    );
    let promoted = position(&|c| c["api"] == "SYNO.Core.Group.Member", "group");
    let scrambled = position(
        &|c| c["name"] == "admin" && c.get("password").is_some(),
        "admin password",
    );
    let expired = position(
        &|c| c["name"] == "admin" && c["expired"] == "now",
        "admin expiry",
    );
    assert!(created < promoted && promoted < scrambled && scrambled < expired);
    assert_eq!(calls[created]["name"], "owner");
    assert_ne!(calls[scrambled]["password"], "Secret-1234");
}

#[tokio::test]
async fn failed_setup_step_stops_with_its_api() {
    let (server, client) = mock_nas().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(path("/webapi/entry.cgi/SYNO.Entry.Request"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "success": true, "data": {
            "has_fail": true,
            "result": [
                { "api": "SYNO.Core.Security.AutoBlock", "method": "set", "success": true },
                { "api": "SYNO.Core.Network", "method": "set", "success": true },
                { "api": "SYNO.Core.User", "method": "create", "success": false, "error": { "code": 3106 } }
            ]
        }})))
        .mount(&server)
        .await;

    let error = client
        .complete_setup(&SetupRequest {
            device_name: "nas".into(),
            account: "admin".into(),
            password: "x".into(),
            time_zone: "Amsterdam".into(),
            updates: UpdatePolicy::NotifyOnly,
            device_label: "test".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(
        error,
        Error::Api {
            api: "SYNO.Core.User create".into(),
            code: 3106
        }
    );
}

#[tokio::test]
async fn only_a_nas_that_hands_out_a_session_needs_setup() {
    let (server, client) = mock_nas().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).insert_header("set-cookie", "id=setup; path=/"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/webapi/entry.cgi"))
        .and(wiremock::matchers::query_param(
            "method",
            "check_permission",
        ))
        .and(wiremock::matchers::header("cookie", "id=setup"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "success": true })))
        .mount(&server)
        .await;
    assert_eq!(
        client.setup_state().await.unwrap(),
        SetupState::NotConfigured
    );

    let (server, client) = mock_nas().await;
    Mock::given(method("GET"))
        .and(path("/webapi/entry.cgi"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "success": false, "error": { "code": 119 } })),
        )
        .mount(&server)
        .await;
    assert_eq!(client.setup_state().await.unwrap(), SetupState::Configured);
}

#[tokio::test]
async fn identity_reads_server_name_and_model_and_tolerates_refusals() {
    let (server, client) = mock_nas().await;
    Mock::given(path("/webapi/entry.cgi"))
        .and(body_string_contains("method=login"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "success": true, "data": { "sid": "s" } })),
        )
        .mount(&server)
        .await;
    Mock::given(path("/webapi/entry.cgi"))
        .and(body_string_contains("api=SYNO.Core.Network&"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "success": false, "error": { "code": 105 } })),
        )
        .mount(&server)
        .await;
    Mock::given(path("/webapi/entry.cgi"))
        .and(body_string_contains("api=SYNO.Core.System&"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "success": true, "data": { "model": "VirtualDSM" } })),
        )
        .mount(&server)
        .await;

    let session = client.login(&credentials()).await.unwrap();
    let (name, model) = client.identity(&session).await;
    assert_eq!(
        name,
        Err(Error::PermissionDenied("SYNO.Core.Network".into()))
    );
    assert_eq!(model, Ok("VirtualDSM".to_owned()));
}
