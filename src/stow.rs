//! Module for STOW-RS requests
use dicom_object::{FileDicomObject, InMemDicomObject};

use futures_util::{stream::BoxStream, Stream, StreamExt};
use rand::{distr::Alphanumeric, Rng};
use reqwest::Body;
use snafu::ResultExt;

use crate::{DicomWebClient, DicomWebError, RequestFailedSnafu};

/// A builder type for STOW-RS requests
pub struct WadoStowRequest {
    client: DicomWebClient,
    url: String,
    instances: BoxStream<'static, Result<Vec<u8>, std::io::Error>>,
}

impl WadoStowRequest {
    fn new(client: DicomWebClient, url: String) -> Self {
        WadoStowRequest {
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

    pub async fn run(self) -> Result<(), DicomWebError> {
        let mut request = self.client.client.post(&self.url);

        // Basic authentication
        if let Some(username) = &self.client.username {
            request = request.basic_auth(username, self.client.password.as_ref());
        }
        // Bearer token
        else if let Some(bearer_token) = &self.client.bearer_token {
            request = request.bearer_auth(bearer_token);
        }

        // Extra headers
        for (key, value) in &self.client.extra_headers {
            request = request.header(key, value);
        }

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

        Ok(())
    }
}

impl DicomWebClient {
    /// Create a STOW-RS request to store instances
    pub fn store_instances(&self) -> WadoStowRequest {
        let url = format!("{}/studies", self.stow_url);
        WadoStowRequest::new(self.clone(), url)
    }

    /// Create a WADO-RS request to retrieve the metadata of a specific study
    pub fn store_instances_in_study(&self, study_instance_uid: &str) -> WadoStowRequest {
        let url = format!("{}/studies/{}", self.stow_url, study_instance_uid);
        WadoStowRequest::new(self.clone(), url)
    }
}
