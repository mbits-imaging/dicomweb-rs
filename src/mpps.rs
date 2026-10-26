//! Module for MPPS-RS requests
//! See https://dicom.nema.org/medical/dicom/current/output/html/part18.html#chapter_15
use dicom_core::Tag;
use dicom_json::DicomJson;
use dicom_object::InMemDicomObject;

use snafu::ResultExt;

use crate::{
    DeserializationFailedSnafu, DicomWebClient, DicomWebError, RequestFailedSnafu,
    apply_auth_and_headers, validate_dicom_json_content_type,
};

/// A builder type for the MPPS-RS Create and Update transactions.
/// The data set must be set for the request to be valid, and will be sent as DICOM JSON.
/// It must contain the attributes required by PS3.4 Table F.7.2-1 for N-CREATE or N-SET respectively.
#[derive(Debug, Clone)]
pub struct MppsSendRequest {
    client: DicomWebClient,
    url: String,
    dataset: Option<InMemDicomObject>,
}

impl MppsSendRequest {
    fn new(client: DicomWebClient, url: String) -> Self {
        MppsSendRequest {
            client,
            url,
            dataset: None,
        }
    }

    /// Execute the MPPS-RS request
    pub async fn run(self) -> Result<(), DicomWebError> {
        let Some(dataset) = &self.dataset else {
            return Err(DicomWebError::Other {
                message: "Data set must be set for MPPS-RS request".to_string(),
            });
        };

        let body = dicom_json::to_vec(dataset).map_err(|e| DicomWebError::Other {
            message: format!("Failed to serialize MPPS data set: {}", e),
        })?;

        let mut request = self
            .client
            .client
            .post(&self.url)
            .header("Content-Type", "application/dicom+json")
            .header("Accept", "application/dicom+json")
            .body(body);
        request = apply_auth_and_headers(request, &self.client);

        let response = request
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

    /// Set the MPPS data set to send. Will be passed as DICOM JSON in the request body.
    pub fn with_dataset(mut self, dataset: InMemDicomObject) -> Self {
        self.dataset = Some(dataset);
        self
    }
}

/// A builder type for the MPPS-RS Retrieve transaction
/// By default, all attributes of the MPPS are returned.
#[derive(Debug, Clone)]
pub struct MppsRetrieveRequest {
    client: DicomWebClient,
    url: String,

    includefields: Vec<Tag>,
}

impl MppsRetrieveRequest {
    fn new(client: DicomWebClient, url: String) -> Self {
        MppsRetrieveRequest {
            client,
            url,
            includefields: vec![],
        }
    }

    /// Execute the MPPS-RS request
    pub async fn run(self) -> Result<InMemDicomObject, DicomWebError> {
        let mut query: Vec<(String, String)> = vec![];
        for include_field in self.includefields.iter() {
            // Convert the tag to a radix string
            let radix_string = format!(
                "{:04x}{:04x}",
                include_field.group(),
                include_field.element()
            );

            query.push((String::from("includefield"), radix_string));
        }

        let mut request = self
            .client
            .client
            .get(&self.url)
            .header("Accept", "application/dicom+json")
            .query(&query);
        request = apply_auth_and_headers(request, &self.client);

        let response = request
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

        Ok(response
            .json::<DicomJson<InMemDicomObject>>()
            .await
            .context(DeserializationFailedSnafu {})?
            .into_inner())
    }

    /// Set the tags that should be returned. Will be passed as a query parameter.
    pub fn with_includefields(mut self, includefields: Vec<Tag>) -> Self {
        self.includefields = includefields;
        self
    }
}

impl DicomWebClient {
    /// Create an MPPS-RS request to create a new Modality Performed Procedure Step
    pub fn create_mpps(&self, mpps_uid: &str) -> MppsSendRequest {
        let base_url = &self.qido_url;
        let url = format!("{base_url}/modality-performed-procedure-steps/{mpps_uid}");

        MppsSendRequest::new(self.clone(), url)
    }

    /// Create an MPPS-RS request to update an existing Modality Performed Procedure Step
    pub fn update_mpps(&self, mpps_uid: &str) -> MppsSendRequest {
        let base_url = &self.qido_url;
        // The update query parameter has no value (PS3.18 §15.5.1)
        let url = format!("{base_url}/modality-performed-procedure-steps/{mpps_uid}?update");

        MppsSendRequest::new(self.clone(), url)
    }

    /// Create an MPPS-RS request to retrieve a Modality Performed Procedure Step
    pub fn retrieve_mpps(&self, mpps_uid: &str) -> MppsRetrieveRequest {
        let base_url = &self.qido_url;
        let url = format!("{base_url}/modality-performed-procedure-steps/{mpps_uid}");

        MppsRetrieveRequest::new(self.clone(), url)
    }
}
