//! Module for STOW-RS requests
//! See https://dicom.nema.org/medical/dicom/current/output/html/part18.html#sect_10.5
use dicom_json::DicomJson;
use dicom_object::{FileDicomObject, InMemDicomObject};

use bytes::Bytes;
use futures_util::{Stream, StreamExt, stream::BoxStream};
use multipart_rs::MultipartStreamWriter;
use reqwest::Body;
use snafu::ResultExt;

use crate::{
    DeserializationFailedSnafu, DicomWebClient, DicomWebError, RequestFailedSnafu,
    apply_auth_and_headers, validate_dicom_json_content_type,
};

/// The byte stream forming the body of a single multipart part.
type InstanceStream = BoxStream<'static, Result<Bytes, std::io::Error>>;

/// A builder type for STOW-RS requests
pub struct StowRequest {
    client: DicomWebClient,
    url: String,
    instances: BoxStream<'static, Result<InstanceStream, std::io::Error>>,
}

impl StowRequest {
    fn new(client: DicomWebClient, url: String) -> Self {
        StowRequest {
            client,
            url,
            instances: futures_util::stream::empty().boxed(),
        }
    }

    /// Send each instance as a stream of byte chunks. This keeps peak memory
    /// bounded by the chunk size instead of the instance size, so prefer this
    /// for instances that are already serialized (e.g. files on disk).
    pub fn with_data_streams<S, B>(mut self, instances: S) -> Self
    where
        S: Stream<Item = B> + Send + 'static,
        B: Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    {
        self.instances = instances.map(|body| Ok(body.boxed())).boxed();
        self
    }

    /// Send each instance from a fully buffered byte vector.
    pub fn with_data(mut self, data: impl Stream<Item = Vec<u8>> + Send + 'static) -> Self {
        self.instances = data
            .map(|buffer| {
                Ok(futures_util::stream::once(async move { Ok(Bytes::from(buffer)) }).boxed())
            })
            .boxed();
        self
    }

    /// Send in-memory DICOM objects, serializing each into a buffer.
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
                Ok(
                    futures_util::stream::once(async move { Ok(Bytes::from(buffer)) }).boxed()
                        as InstanceStream,
                )
            })
            .boxed();
        self
    }

    pub async fn run(self) -> Result<InMemDicomObject, DicomWebError> {
        let mut request = self.client.client.post(&self.url);
        request = apply_auth_and_headers(request, &self.client);

        let writer = MultipartStreamWriter::new();

        let request = request.header(
            "Content-Type",
            format!(
                "multipart/related; type=\"application/dicom\"; boundary={}",
                writer.boundary
            ),
        );

        // Convert each instance's chunk stream to a multipart part
        let parts = self.instances.map(|body| {
            Ok::<_, std::io::Error>(("Content-Type: application/dicom".to_string(), body?))
        });
        let multipart_stream = writer.stream(parts);

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
