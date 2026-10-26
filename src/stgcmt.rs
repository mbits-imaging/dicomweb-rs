//! Module for Storage Commitment requests
//! See https://dicom.nema.org/medical/dicom/current/output/html/part18.html#chapter_13
use dicom_core::value::DataSetSequence;
use dicom_core::{DataElement, PrimitiveValue, VR};
use dicom_dictionary_std::tags;
use dicom_json::DicomJson;
use dicom_object::InMemDicomObject;
use reqwest::{Response, StatusCode};

use snafu::ResultExt;

use crate::{
    DeserializationFailedSnafu, DicomWebClient, DicomWebError, RequestFailedSnafu,
    apply_auth_and_headers, validate_dicom_json_content_type,
};

/// The state of a storage commitment request
#[derive(Debug, Clone)]
pub enum CommitmentStatus {
    /// The origin server has not finished processing the request yet (202 Accepted).
    /// The result should be checked again later.
    Pending,
    /// The origin server finished processing the request (200 OK).
    /// Contains the Storage Commitment Response Module (PS3.18 Annex J.2),
    /// which lists the committed and failed instances.
    Done(InMemDicomObject),
}

/// Converts a Commit or Check Commit Result response into a [`CommitmentStatus`]
async fn commitment_status(response: Response) -> Result<CommitmentStatus, DicomWebError> {
    if response.status() == StatusCode::ACCEPTED {
        return Ok(CommitmentStatus::Pending);
    }

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

    Ok(CommitmentStatus::Done(
        response
            .json::<DicomJson<InMemDicomObject>>()
            .await
            .context(DeserializationFailedSnafu {})?
            .into_inner(),
    ))
}

/// A builder type for the Storage Commitment Commit transaction
/// The instances to commit must be set for the request to be valid,
/// either with [`CommitRequest::with_instance`] or with a complete request data set.
#[derive(Debug, Clone)]
pub struct CommitRequest {
    client: DicomWebClient,
    url: String,
    instances: Vec<(String, String)>,
    dataset: Option<InMemDicomObject>,
}

impl CommitRequest {
    fn new(client: DicomWebClient, url: String) -> Self {
        CommitRequest {
            client,
            url,
            instances: vec![],
            dataset: None,
        }
    }

    /// Execute the Storage Commitment request
    pub async fn run(self) -> Result<CommitmentStatus, DicomWebError> {
        let dataset = match self.dataset {
            Some(dataset) => dataset,
            None if !self.instances.is_empty() => {
                // Build the Referenced SOP Sequence of the Storage Commitment Request Module
                let items: Vec<InMemDicomObject> = self
                    .instances
                    .iter()
                    .map(|(sop_class_uid, sop_instance_uid)| {
                        InMemDicomObject::from_element_iter([
                            DataElement::new(
                                tags::REFERENCED_SOP_CLASS_UID,
                                VR::UI,
                                PrimitiveValue::from(sop_class_uid.as_str()),
                            ),
                            DataElement::new(
                                tags::REFERENCED_SOP_INSTANCE_UID,
                                VR::UI,
                                PrimitiveValue::from(sop_instance_uid.as_str()),
                            ),
                        ])
                    })
                    .collect();
                InMemDicomObject::from_element_iter([DataElement::new(
                    tags::REFERENCED_SOP_SEQUENCE,
                    VR::SQ,
                    DataSetSequence::from(items),
                )])
            }
            None => {
                return Err(DicomWebError::Other {
                    message: "Instances must be set for Storage Commitment request".to_string(),
                });
            }
        };

        let body = dicom_json::to_vec(&dataset).map_err(|e| DicomWebError::Other {
            message: format!("Failed to serialize Storage Commitment request: {}", e),
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

        commitment_status(response).await
    }

    /// Add an instance to commit, identified by its SOP Class UID and SOP Instance UID.
    /// Will be passed in the Referenced SOP Sequence of the request.
    pub fn with_instance(mut self, sop_class_uid: &str, sop_instance_uid: &str) -> Self {
        self.instances
            .push((sop_class_uid.to_string(), sop_instance_uid.to_string()));
        self
    }

    /// Set the complete Storage Commitment Request Module (PS3.18 Annex J.1) to send,
    /// e.g. to reference the instances by the Referenced Study Sequence.
    /// Takes precedence over instances added with [`CommitRequest::with_instance`].
    pub fn with_dataset(mut self, dataset: InMemDicomObject) -> Self {
        self.dataset = Some(dataset);
        self
    }
}

/// A builder type for the Storage Commitment Check Commit Result transaction
#[derive(Debug, Clone)]
pub struct CommitResultRequest {
    client: DicomWebClient,
    url: String,
}

impl CommitResultRequest {
    fn new(client: DicomWebClient, url: String) -> Self {
        CommitResultRequest { client, url }
    }

    /// Execute the Storage Commitment request
    pub async fn run(self) -> Result<CommitmentStatus, DicomWebError> {
        let mut request = self
            .client
            .client
            .get(&self.url)
            .header("Accept", "application/dicom+json");
        request = apply_auth_and_headers(request, &self.client);

        let response = request
            .send()
            .await
            .context(RequestFailedSnafu { url: &self.url })?;

        commitment_status(response).await
    }
}

impl DicomWebClient {
    /// Create a Storage Commitment request to commit instances for safekeeping
    pub fn request_storage_commitment(&self, transaction_uid: &str) -> CommitRequest {
        let base_url = &self.stow_url;
        let url = format!("{base_url}/commitment-requests/{transaction_uid}");

        CommitRequest::new(self.clone(), url)
    }

    /// Create a Storage Commitment request to check the result of an earlier commit request
    pub fn check_storage_commitment(&self, transaction_uid: &str) -> CommitResultRequest {
        let base_url = &self.stow_url;
        let url = format!("{base_url}/commitment-requests/{transaction_uid}");

        CommitResultRequest::new(self.clone(), url)
    }
}
