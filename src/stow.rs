//! Module for WADO-RS requests
use dicom_object::{FileDicomObject, InMemDicomObject};

use rand::{distr::Alphanumeric, Rng};
use snafu::ResultExt;

use crate::{DicomWebClient, DicomWebError, RequestFailedSnafu};

/// A builder type for STOW-RS requests
#[derive(Debug, Clone)]
pub struct WadoStowRequest {
    client: DicomWebClient,
    url: String,
    instances: Vec<FileDicomObject<InMemDicomObject>>,
}

impl WadoStowRequest {
    fn new(client: DicomWebClient, url: String) -> Self {
        WadoStowRequest {
            client,
            url,
            instances: Vec::new(),
        }
    }

    pub fn with_instances(mut self, instances: Vec<FileDicomObject<InMemDicomObject>>) -> Self {
        self.instances = instances;
        self
    }

    pub async fn run(&self) -> Result<(), DicomWebError> {
        let mut request = self.client.client.post(&self.url);

        // Basic authentication
        if let Some(username) = &self.client.username {
            request = request.basic_auth(username, self.client.password.as_ref());
        }
        // Bearer token
        else if let Some(bearer_token) = &self.client.bearer_token {
            request = request.bearer_auth(bearer_token);
        }

        let boundary: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(8)
            .map(char::from)
            .collect();

        let mut multipart_buffer = vec![];
        for instance in &self.instances {
            let mut buffer = Vec::new();
            instance.write_all(&mut buffer).unwrap();
            multipart_buffer.extend_from_slice(b"--");
            multipart_buffer.extend_from_slice(boundary.as_bytes());
            multipart_buffer.extend_from_slice(b"\r\n");
            multipart_buffer.extend_from_slice(b"Content-Type: application/dicom\r\n\r\n");

            multipart_buffer.extend_from_slice(&buffer);
            multipart_buffer.extend_from_slice(b"\r\n");
        }
        // Write the final boundary
        multipart_buffer.extend_from_slice(b"--");
        multipart_buffer.extend_from_slice(boundary.as_bytes());
        multipart_buffer.extend_from_slice(b"--\r\n");

        let response = request
            .header(
                "Content-Type",
                format!("multipart/related; boundary={}", boundary),
            )
            .body(multipart_buffer)
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
