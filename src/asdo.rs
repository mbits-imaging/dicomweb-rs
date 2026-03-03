//! Module for ASDO-RS requests
//! See https://www.dicomstandard.org/News-dir/ftsup/docs/sups/sup248.pdf
use dicom_core::ops::AttributeSelector;
use dicom_json::DicomJson;
use dicom_object::InMemDicomObject;

use serde::{Deserialize, Serialize};
use snafu::ResultExt;

use crate::{
    apply_auth_and_headers, selector_to_string, validate_dicom_json_content_type,
    DeserializationFailedSnafu, DicomWebClient, DicomWebError, RequestFailedSnafu,
};

/// A builder type for ASDO-RS requests
/// By default, the request is built with no filters, no limit, and no offset.
#[derive(Debug, Clone)]
pub struct AsdoSendRequest {
    client: DicomWebClient,
    url: String,
    destination: String,
    // These are an extension for the ASDO-RS request, not part of the standard.
    // They will be sent as a json body in the request, and can be used to provide authentication information for the destination.
    username: Option<String>,
    password: Option<String>,
    token: Option<String>,

    filters: Vec<(AttributeSelector, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthInfo {
    username: Option<String>,
    password: Option<String>,
    token: Option<String>,
}

impl AsdoSendRequest {
    fn new(client: DicomWebClient, url: String) -> Self {
        AsdoSendRequest {
            client,
            url,
            filters: vec![],
            destination: String::new(),
            username: None,
            password: None,
            token: None,
        }
    }

    /// Execute the ASDO-RS request
    pub async fn run(self) -> Result<InMemDicomObject, DicomWebError> {
        let mut query: Vec<(String, String)> = vec![];
        for (selector, value) in self.filters.iter() {
            query.push((selector_to_string(&selector), value.clone()));
        }

        query.push((String::from("destination"), self.destination.clone()));

        let mut request = self.client.client.post(&self.url).query(&query);
        // Forward the authentication information in the body of the request,
        // since ASDO-RS does not have a standard way to provide authentication information for the destination.
        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.json(&AuthInfo {
                username: Some(username.clone()),
                password: Some(password.clone()),
                token: None,
            });
        } else if let Some(token) = &self.token {
            request = request.json(&AuthInfo {
                username: None,
                password: None,
                token: Some(token.clone()),
            });
        }

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

    /// Add a filter to the query. Will be passed as a query parameter.
    pub fn with_filter(mut self, selector: AttributeSelector, value: String) -> Self {
        self.filters.push((selector, value));
        self
    }

    /// Set the destination for the ASDO-RS request. Will be passed as a query parameter.
    pub fn with_destination(mut self, destination: String) -> Self {
        self.destination = destination;
        self
    }

    pub fn with_basic_auth(mut self, username: String, password: String) -> Self {
        self.username = Some(username);
        self.password = Some(password);
        self
    }

    pub fn with_bearer_token(mut self, token: String) -> Self {
        self.token = Some(token);
        self
    }
}

#[derive(Debug, Clone)]
pub struct AsdoStatusRequest {
    client: DicomWebClient,
    url: String,
}

impl AsdoStatusRequest {
    fn new(client: DicomWebClient, url: String) -> Self {
        AsdoStatusRequest { client, url }
    }

    pub async fn run(&self) -> Result<InMemDicomObject, DicomWebError> {
        let request = self.client.client.get(&self.url);
        let request = apply_auth_and_headers(request, &self.client);

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
}

impl DicomWebClient {
    /// Create an ASDO-RS request to send all studies
    pub fn send_studies(&self, transaction_uid: &str) -> AsdoSendRequest {
        let base_url = &self.qido_url;
        let url = format!("{base_url}/studies/send-requests/{transaction_uid}");

        AsdoSendRequest::new(self.clone(), url)
    }

    /// Create an ASDO-RS request to retrieve the status of a send request for all studies
    pub fn send_studies_status(&self, transaction_uid: &str) -> AsdoStatusRequest {
        let base_url = &self.qido_url;
        let url = format!("{base_url}/studies/send-requests/{transaction_uid}");

        AsdoStatusRequest::new(self.clone(), url)
    }

    /// Create an ASDO-RS request to send all series in a specific study
    pub fn send_series_in_study(
        &self,
        study_instance_uid: &str,
        transaction_uid: &str,
    ) -> AsdoSendRequest {
        let base_url = &self.qido_url;
        let url = format!(
            "{base_url}/studies/{study_instance_uid}/series/send-requests/{transaction_uid}"
        );

        AsdoSendRequest::new(self.clone(), url)
    }

    /// Create an ASDO-RS request to send all instances in a specific study
    pub fn send_instances_in_study(
        &self,
        study_instance_uid: &str,
        transaction_uid: &str,
    ) -> AsdoSendRequest {
        let base_url = &self.qido_url;
        let url = format!(
            "{base_url}/studies/{study_instance_uid}/instances/send-requests/{transaction_uid}"
        );

        AsdoSendRequest::new(self.clone(), url)
    }

    /// Create an ASDO-RS request to send all instances in a specific series
    pub fn send_instances_in_series(
        &self,
        study_instance_uid: &str,
        series_instance_uid: &str,
        transaction_uid: &str,
    ) -> AsdoSendRequest {
        let base_url = &self.qido_url;
        let url = format!(
            "{base_url}/studies/{study_instance_uid}/series/{series_instance_uid}/instances/send-requests/{transaction_uid}",
        );

        AsdoSendRequest::new(self.clone(), url)
    }
}
