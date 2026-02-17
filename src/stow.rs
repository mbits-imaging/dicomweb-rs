//! Module for STOW-RS requests
//! See https://dicom.nema.org/medical/dicom/current/output/html/part18.html#sect_10.5
use dicom_json::DicomJson;
use dicom_object::{FileDicomObject, InMemDicomObject};

use futures_util::{stream::BoxStream, Stream, StreamExt};
use rand::{distr::Alphanumeric, Rng};
use reqwest::Body;
use snafu::ResultExt;

use crate::{
    apply_auth_and_headers, validate_dicom_json_content_type, DeserializationFailedSnafu,
    DicomWebClient, DicomWebError, RequestFailedSnafu,
};

/// A builder type for STOW-RS requests
pub struct StowRequest {
    client: DicomWebClient,
    url: String,
    instances: BoxStream<'static, Result<Vec<u8>, std::io::Error>>,
}

impl StowRequest {
    fn new(client: DicomWebClient, url: String) -> Self {
        StowRequest {
            client,
            url,
            instances: futures_util::stream::empty().boxed(),
        }
    }

    pub fn with_data(mut self, data: impl Stream<Item = Vec<u8>> + Send + 'static) -> Self {
        self.instances = data.map(Ok).boxed();
        self
    }

    pub fn with_instances(
        mut self,
        instances: impl Stream<Item = FileDicomObject<InMemDicomObject>> + Send + 'static,
    ) -> Self {
        self.instances = instances
            .map(|instance| {
                let mut buffer = Vec::new();
                instance.write_all(&mut buffer).map_err(|e| {
                    std::io::Error::other(format!("Failed to serialize DICOM instance: {}", e))
                })?;
                Ok(buffer)
            })
            .boxed();
        self
    }

    pub async fn run(self) -> Result<InMemDicomObject, DicomWebError> {
        let mut request = self.client.client.post(&self.url);
        request = apply_auth_and_headers(request, &self.client);

        let boundary: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(8)
            .map(char::from)
            .collect();

        let request = request.header(
            "Content-Type",
            format!(
                "multipart/related; type=\"application/dicom\"; boundary={}",
                boundary
            ),
        );

        let boundary_clone = boundary.clone();

        // Convert each instance to a multipart item
        let multipart_stream = self.instances.map(move |data| {
            let mut multipart_item = Vec::new();
            let buffer = data?;
            multipart_item.extend_from_slice(b"--");
            multipart_item.extend_from_slice(boundary.as_bytes());
            multipart_item.extend_from_slice(b"\r\n");
            multipart_item.extend_from_slice(b"Content-Type: application/dicom\r\n\r\n");
            multipart_item.extend_from_slice(&buffer);
            multipart_item.extend_from_slice(b"\r\n");
            Ok::<_, std::io::Error>(multipart_item)
        });

        // Write the final boundary
        let multipart_stream = multipart_stream.chain(futures_util::stream::once(async move {
            Ok(format!("--{}--\r\n", boundary_clone).into_bytes())
        }));

        let response = request
            .body(Body::wrap_stream(multipart_stream))
            .send()
            .await
            .context(RequestFailedSnafu { url: &self.url })?;

        if !response.status().is_success() {
            return Err(DicomWebError::HttpStatusFailure {
                status_code: response.status(),
            });
        }

        // Check if the response is a DICOM-JSON
        let ct = response
            .headers()
            .get("Content-Type")
            .ok_or(DicomWebError::MissingContentTypeHeader)?;
        validate_dicom_json_content_type(ct.to_str().unwrap_or_default())?;

        // STOW-RS response is a single DICOM JSON dataset (PS3.18 §10.5.1)
        Ok(response
            .json::<DicomJson<InMemDicomObject>>()
            .await
            .context(DeserializationFailedSnafu {})?
            .into_inner())
    }
}

impl DicomWebClient {
    /// Create a STOW-RS request to store instances
    pub fn store_instances(&self) -> StowRequest {
        let url = format!("{}/studies", self.stow_url);
        StowRequest::new(self.clone(), url)
    }

    /// Create a STOW-RS request to store instances in a specific study
    pub fn store_instances_in_study(&self, study_instance_uid: &str) -> StowRequest {
        let url = format!("{}/studies/{}", self.stow_url, study_instance_uid);
        StowRequest::new(self.clone(), url)
    }
}
