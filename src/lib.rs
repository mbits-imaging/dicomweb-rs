//! This crate contains a DICOMweb client for querying and retrieving DICOM objects.
//!
//! It supports the QIDO-RS and WADO-RS DICOMweb services, which are used to query and retrieve DICOM objects respectively.
//! As of now, the STOW-RS service is not supported.
//! The HTTP requests are made using the reqwest crate, which is a high-level HTTP client for Rust.
//!
//! # Examples
//!
//! Query all studies from a DICOMweb server (with authentication):
//!
//! ```no_run
//! use dicom_dictionary_std::tags;
//! use dicom_web::DicomWebClient;
//!
//! async fn foo()
//! {
//!   let mut client = DicomWebClient::with_single_url("http://localhost:8042");
//!   client.set_basic_auth("orthanc", "orthanc");
//!
//!   let studies = client.query_studies().run().await.unwrap();
//!
//!   for study in studies {
//!       let study_instance_uid = study.element(tags::STUDY_INSTANCE_UID).unwrap().to_str().unwrap();
//!       println!("Study: {}", study_instance_uid);
//!   }
//! }
//! ```
//!
//! To retrieve a DICOM study from a DICOMweb server:
//! ```no_run
//! use dicom_dictionary_std::tags;
//! use dicom_web::DicomWebClient;
//! use futures_util::StreamExt;
//!
//! async fn foo()
//! {
//!   let mut client = DicomWebClient::with_single_url("http://localhost:8042");
//!   client.set_basic_auth("orthanc", "orthanc");
//!   
//!   let study_instance_uid = "1.2.276.0.89.300.10035584652.20181014.93645";
//!   
//!   let mut study_objects = client.retrieve_study(study_instance_uid).run().await.unwrap();
//!
//!   while let Some(object) = study_objects.next().await {
//!       let object = object.unwrap();
//!       let sop_instance_uid = object.element(tags::SOP_INSTANCE_UID).unwrap().to_str().unwrap();
//!       println!("Instance: {}", sop_instance_uid);
//!   }
//! }
//! ```
use mediatype::MediaTypeError;
use multipart_rs::MultipartType;
use reqwest::StatusCode;
use snafu::Snafu;
use std::collections::HashMap;

mod mwl;
mod qido;
mod stow;
mod wado;
/// The DICOMweb client for querying and retrieving DICOM objects.
/// Can be reused for multiple requests.
#[derive(Debug, Clone)]
pub struct DicomWebClient {
    wado_url: String,
    qido_url: String,
    stow_url: String,

    // Basic Auth
    pub(crate) username: Option<String>,
    pub(crate) password: Option<String>,
    // Bearer Token
    pub(crate) bearer_token: Option<String>,
    // Headers
    pub(crate) extra_headers: HashMap<String, String>,

    pub(crate) client: reqwest::Client,
}

/// An error returned when parsing an invalid tag range.
#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
pub enum DicomWebError {
    #[snafu(display("Failed to perform HTTP request"))]
    RequestFailed { url: String, source: reqwest::Error },
    #[snafu(display("Failed to deserialize response from server"))]
    DeserializationFailed { source: reqwest::Error },
    #[snafu(display("Failed to parse multipart response"))]
    MultipartReaderFailed {
        source: multipart_rs::MultipartError,
    },
    #[snafu(display("Failed to read DICOM object from multipart item"))]
    DicomReaderFailed { source: dicom_object::ReadError },
    #[snafu(display("HTTP status code indicates failure"))]
    HttpStatusFailure { status_code: StatusCode },
    #[snafu(display("Multipart item missing Content-Type header"))]
    MissingContentTypeHeader,
    #[snafu(display("Unexpected content type: {}", content_type))]
    UnexpectedContentType { content_type: String },
    #[snafu(display("Failed to parse content type: {}", source))]
    ContentTypeParseFailed { source: MediaTypeError },
    #[snafu(display("Unexpected multipart type: {:?}", multipart_type))]
    UnexpectedMultipartType { multipart_type: MultipartType },
    #[snafu(display("Empty response"))]
    EmptyResponse,
}

impl DicomWebClient {
    /// Set the basic authentication for the DICOMWeb client. Will be passed in the Authorization header.
    pub fn set_basic_auth(&mut self, username: &str, password: &str) -> &Self {
        self.username = Some(username.to_string());
        self.password = Some(password.to_string());
        self
    }

    /// Set the bearer token for the DICOMWeb client. Will be passed in the Authorization header.
    pub fn set_bearer_token(&mut self, token: &str) -> &Self {
        self.bearer_token = Some(token.to_string());
        self
    }

    pub fn add_header(&mut self, key: &str, value: &str) -> &Self {
        self.extra_headers
            .insert(key.to_string(), value.to_string());
        self
    }

    /// Create a new DICOMWeb client with the same URL for all services (WADO-RS, QIDO-RS, STOW-RS).
    pub fn with_single_url(url: &str) -> DicomWebClient {
        DicomWebClient {
            wado_url: url.to_string(),
            qido_url: url.to_string(),
            stow_url: url.to_string(),
            client: reqwest::Client::new(),
            extra_headers: HashMap::new(),
            bearer_token: None,
            username: None,
            password: None,
        }
    }

    /// Create a new DICOMWeb client with separate URLs for each service.
    pub fn with_separate_urls(wado_url: &str, qido_url: &str, stow_url: &str) -> DicomWebClient {
        DicomWebClient {
            wado_url: wado_url.to_string(),
            qido_url: qido_url.to_string(),
            stow_url: stow_url.to_string(),
            extra_headers: HashMap::new(),
            client: reqwest::Client::new(),
            bearer_token: None,
            username: None,
            password: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use dicom_dictionary_std::uids;
    use dicom_object::{FileMetaTableBuilder, InMemDicomObject};
    use serde_json::json;
    use wiremock::MockServer;

    use super::*;

    async fn mock_qido(mock_server: &MockServer) {
        // STUDIES endpoint
        let mock = wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::header_exists("Accept"))
            .and(wiremock::matchers::path("/studies"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!([])));
        mock_server.register(mock).await;
        // SERIES endpoint
        let mock = wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::header_exists("Accept"))
            .and(wiremock::matchers::path("/series"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!([])));
        mock_server.register(mock).await;
        // INSTANCES endpoint
        let mock = wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::header_exists("Accept"))
            .and(wiremock::matchers::path("/instances"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!([])));
        mock_server.register(mock).await;
        // STUDIES/{STUDY_UID}/SERIES endpoint
        let mock = wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::header_exists("Accept"))
            .and(wiremock::matchers::path_regex("^/studies/[0-9.]+/series$"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!([])));
        mock_server.register(mock).await;
        // STUDIES/{STUDY_UID}/SERIES/{SERIES_UID}/INSTANCES endpoint
        let mock = wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::header_exists("Accept"))
            .and(wiremock::matchers::path_regex(
                "^/studies/[0-9.]+/series/[0-9.]+/instances$",
            ))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!([])));
        mock_server.register(mock).await;
    }

    async fn mock_wado(mock_server: &MockServer) {
        let dcm_multipart_response = wiremock::ResponseTemplate::new(200).set_body_raw(
            "--1234\r\nContent-Type: application/dicom\r\n\r\n--1234--",
            "multipart/related; boundary=1234",
        );

        // STUDIES/{STUDY_UID} endpoint
        let mock = wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::header_exists("Accept"))
            .and(wiremock::matchers::path_regex("^/studies/[0-9.]+$"))
            .respond_with(dcm_multipart_response.clone());
        mock_server.register(mock).await;
        // STUDIES/{STUDY_UID}/METADATA endpoint
        let mock = wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::header_exists("Accept"))
            .and(wiremock::matchers::path_regex(
                "^/studies/[0-9.]+/metadata$",
            ))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_raw("[]", "application/dicom+json"),
            );
        mock_server.register(mock).await;
        // STUDIES/{STUDY_UID}/SERIES/{SERIES_UID} endpoint
        let mock = wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::header_exists("Accept"))
            .and(wiremock::matchers::path_regex(
                r"^/studies/[0-9.]+/series/[0-9.]+$",
            ))
            .respond_with(dcm_multipart_response.clone());
        mock_server.register(mock).await;
        // STUDIES/{STUDY_UID}/SERIES/{SERIES_UID}/METADATA endpoint
        let mock = wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::header_exists("Accept"))
            .and(wiremock::matchers::path_regex(
                r"^/studies/[0-9.]+/series/[0-9.]+/metadata$",
            ))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_raw("[]", "application/dicom+json"),
            );
        mock_server.register(mock).await;
        // STUDIES/{STUDY_UID}/SERIES/{SERIES_UID}/INSTANCES/{INSTANCE_UID} endpoint
        let mock = wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::header_exists("Accept"))
            .and(wiremock::matchers::path_regex(
                r"^/studies/[0-9.]+/series/[0-9.]+/instances/[0-9.]+$",
            ))
            .respond_with(dcm_multipart_response.clone());
        mock_server.register(mock).await;
        // STUDIES/{STUDY_UID}/SERIES/{SERIES_UID}/INSTANCES/{INSTANCE_UID}/METADATA endpoint
        let mock = wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::header_exists("Accept"))
            .and(wiremock::matchers::path_regex(
                r"^/studies/[0-9.]+/series/[0-9.]+/instances/[0-9.]+/metadata$",
            ))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_raw("[]", "application/dicom+json"),
            );
        mock_server.register(mock).await;
        // STUDIES/{STUDY_UID}/SERIES/{SERIES_UID}/INSTANCES/{INSTANCE_UID}/frames/{framelist} endpoint
        let mock = wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::header_exists("Accept"))
            .and(wiremock::matchers::path_regex(
                r"^/studies/[0-9.]+/series/[0-9.]+/instances/[0-9.]+/frames/[0-9,]+$",
            ))
            .respond_with(dcm_multipart_response);
        mock_server.register(mock).await;
    }

    async fn mock_stow(mock_server: &MockServer) {
        // STUDIES endpoint for STOW-RS
        let mock = wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::header_exists("Content-Type"))
            .and(wiremock::matchers::path("/studies"))
            .respond_with(wiremock::ResponseTemplate::new(201));
        mock_server.register(mock).await;
    }

    // Create a DICOMWeb mock server
    async fn start_dicomweb_mock_server() -> MockServer {
        let mock_server = MockServer::start().await;
        mock_qido(&mock_server).await;
        mock_wado(&mock_server).await;
        mock_stow(&mock_server).await;
        mock_server
    }

    #[tokio::test]
    async fn query_study_test() {
        let mock_server = start_dicomweb_mock_server().await;
        let client = DicomWebClient::with_single_url(&mock_server.uri());
        // Perform QIDO-RS request
        let result = client.query_studies().run().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn query_series_test() {
        let mock_server = start_dicomweb_mock_server().await;
        let client = DicomWebClient::with_single_url(&mock_server.uri());
        // Perform QIDO-RS request
        let result = client.query_series().run().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn query_instances_test() {
        let mock_server = start_dicomweb_mock_server().await;
        let client = DicomWebClient::with_single_url(&mock_server.uri());
        // Perform QIDO-RS request
        let result = client.query_instances().run().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn query_series_in_study_test() {
        let mock_server = start_dicomweb_mock_server().await;
        let client = DicomWebClient::with_single_url(&mock_server.uri());
        // Perform QIDO-RS request
        let result = client
            .query_series_in_study("1.2.276.0.89.300.10035584652.20181014.93645")
            .run()
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn query_instances_in_series_test() {
        let mock_server = start_dicomweb_mock_server().await;
        let client = DicomWebClient::with_single_url(&mock_server.uri());
        // Perform QIDO-RS request
        let result = client
            .query_instances_in_series("1.2.276.0.89.300.10035584652.20181014.93645", "1.1.1.1")
            .run()
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn retrieve_study_test() {
        let mock_server = start_dicomweb_mock_server().await;
        let client = DicomWebClient::with_single_url(&mock_server.uri());
        // Perform WADO-RS request
        let result = client
            .retrieve_study("1.2.276.0.89.300.10035584652.20181014.93645")
            .run()
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn retrieve_study_metadata_test() {
        let mock_server = start_dicomweb_mock_server().await;
        let client = DicomWebClient::with_single_url(&mock_server.uri());
        // Perform WADO-RS request
        let result = client
            .retrieve_study_metadata("1.2.276.0.89.300.10035584652.20181014.93645")
            .run()
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn retrieve_series_test() {
        let mock_server = start_dicomweb_mock_server().await;
        let client = DicomWebClient::with_single_url(&mock_server.uri());
        // Perform WADO-RS request
        let result = client
            .retrieve_series(
                "1.2.276.0.89.300.10035584652.20181014.93645",
                "1.2.392.200036.9125.3.1696751121028.64888163108.42362053",
            )
            .run()
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn retrieve_series_metadata_test() {
        let mock_server = start_dicomweb_mock_server().await;
        let client = DicomWebClient::with_single_url(&mock_server.uri());
        // Perform WADO-RS request
        let result = client
            .retrieve_series_metadata(
                "1.2.276.0.89.300.10035584652.20181014.93645",
                "1.2.392.200036.9125.3.1696751121028.64888163108.42362053",
            )
            .run()
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn retrieve_instance_test() {
        let mock_server = start_dicomweb_mock_server().await;
        let client = DicomWebClient::with_single_url(&mock_server.uri());
        // Perform WADO-RS request
        let result = client
            .retrieve_instance(
                "1.2.276.0.89.300.10035584652.20181014.93645",
                "1.2.392.200036.9125.3.1696751121028.64888163108.42362053",
                "1.2.392.200036.9125.9.0.454007928.521494544.1883970570",
            )
            .run()
            .await;
        assert!(result.is_err_and(|e| e.to_string().contains("Empty")));
    }

    #[tokio::test]
    async fn retrieve_instance_metadata_test() {
        let mock_server = start_dicomweb_mock_server().await;
        let client = DicomWebClient::with_single_url(&mock_server.uri());
        // Perform WADO-RS request
        let result = client
            .retrieve_instance_metadata(
                "1.2.276.0.89.300.10035584652.20181014.93645",
                "1.2.392.200036.9125.3.1696751121028.64888163108.42362053",
                "1.2.392.200036.9125.9.0.454007928.521494544.1883970570",
            )
            .run()
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn retrieve_frames_test() {
        let mock_server = start_dicomweb_mock_server().await;
        let mut client = DicomWebClient::with_single_url(&mock_server.uri());
        client.set_basic_auth("orthanc", "orthanc");
        // Perform WADO-RS request
        let result = client
            .retrieve_frames(
                "1.2.276.0.89.300.10035584652.20181014.93645",
                "1.2.392.200036.9125.3.1696751121028.64888163108.42362053",
                "1.2.392.200036.9125.9.0.454007928.521494544.1883970570",
                &[1],
            )
            .run()
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn store_instances_test() {
        let mock_server = start_dicomweb_mock_server().await;
        let mut client = DicomWebClient::with_single_url(&mock_server.uri());
        client.set_basic_auth("orthanc", "orthanc");
        // Create new empty DICOM instance
        let instance = InMemDicomObject::new_empty()
            .with_meta(
                FileMetaTableBuilder::new()
                    // Implicit VR Little Endian
                    .transfer_syntax(uids::IMPLICIT_VR_LITTLE_ENDIAN)
                    // Computed Radiography image storage
                    .media_storage_sop_class_uid("1.2.840.10008.5.1.4.1.1.1"),
            )
            .unwrap();
        // Create a stream with the instance
        let stream = futures_util::stream::once(async move { instance });

        // Perform WADO-RS request
        let result = client.store_instances().with_instances(stream).run().await;
        assert!(result.is_ok());
    }
}
