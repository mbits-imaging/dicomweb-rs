//! Module for QIDO-RS requests
//! See https://dicom.nema.org/medical/dicom/current/output/html/part18.html#sect_10.6
use dicom_core::{ops::AttributeSelector, Tag};
use dicom_json::DicomJson;
use dicom_object::InMemDicomObject;

use snafu::ResultExt;

use crate::{
    apply_auth_and_headers, selector_to_string, validate_dicom_json_content_type,
    DeserializationFailedSnafu, DicomWebClient, DicomWebError, RequestFailedSnafu,
};

/// A builder type for QIDO-RS requests
/// By default, the request is built with no filters, no limit, and no offset.
#[derive(Debug, Clone)]
pub struct QidoRequest {
    client: DicomWebClient,
    url: String,

    limit: Option<u32>,
    offset: Option<u32>,
    includefields: Vec<Tag>,
    fuzzymatching: Option<bool>,
    filters: Vec<(AttributeSelector, String)>,
}

impl QidoRequest {
    fn new(client: DicomWebClient, url: String) -> Self {
        QidoRequest {
            client,
            url,
            limit: None,
            offset: None,
            includefields: vec![],
            fuzzymatching: None,
            filters: vec![],
        }
    }

    /// Execute the QIDO-RS request
    pub async fn run(self) -> Result<Vec<InMemDicomObject>, DicomWebError> {
        let mut query: Vec<(String, String)> = vec![];
        if let Some(limit) = self.limit {
            query.push((String::from("limit"), limit.to_string()));
        }
        if let Some(offset) = self.offset {
            query.push((String::from("offset"), offset.to_string()));
        }
        if let Some(fuzzymatching) = self.fuzzymatching {
            query.push((String::from("fuzzymatching"), fuzzymatching.to_string()));
        }
        for include_field in self.includefields.iter() {
            // Convert the tag to a radix string
            let radix_string = format!(
                "{:04x}{:04x}",
                include_field.group(),
                include_field.element()
            );

            query.push((String::from("includefield"), radix_string));
        }
        for (selector, value) in self.filters.iter() {
            query.push((selector_to_string(&selector), value.clone()));
        }

        let mut request = self.client.client.get(&self.url).query(&query);
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
            .json::<Vec<DicomJson<InMemDicomObject>>>()
            .await
            .context(DeserializationFailedSnafu {})?
            .into_iter()
            .map(|dicomjson| dicomjson.into_inner())
            .collect())
    }

    /// Set the maximum number of results to return. Will be passed as a query parameter.
    /// This is useful for pagination.
    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set the offset of the results to return. Will be passed as a query parameter.
    /// This is useful for pagination.
    pub fn with_offset(mut self, offset: u32) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Set the tags that should be queried. Will be passed as a query parameter.
    pub fn with_includefields(mut self, includefields: Vec<Tag>) -> Self {
        self.includefields = includefields;
        self
    }

    /// Set whether fuzzy matching should be used. Will be passed as a query parameter.
    pub fn with_fuzzymatching(mut self, fuzzymatching: bool) -> Self {
        self.fuzzymatching = Some(fuzzymatching);
        self
    }

    /// Add a filter to the query. Will be passed as a query parameter.
    pub fn with_filter(mut self, selector: AttributeSelector, value: String) -> Self {
        self.filters.push((selector, value));
        self
    }
}

impl DicomWebClient {
    /// Create a QIDO-RS request to query all studies
    pub fn query_studies(&self) -> QidoRequest {
        let base_url = &self.qido_url;
        let url = format!("{base_url}/studies");

        QidoRequest::new(self.clone(), url)
    }

    /// Create a QIDO-RS request to query all series
    pub fn query_series(&self) -> QidoRequest {
        let base_url = &self.qido_url;
        let url = format!("{base_url}/series");

        QidoRequest::new(self.clone(), url)
    }

    /// Create a QIDO-RS request to query all series in a specific study
    pub fn query_series_in_study(&self, study_instance_uid: &str) -> QidoRequest {
        let base_url = &self.qido_url;
        let url = format!("{base_url}/studies/{study_instance_uid}/series");

        QidoRequest::new(self.clone(), url)
    }

    /// Create a QIDO-RS request to query all instances
    pub fn query_instances(&self) -> QidoRequest {
        let base_url = &self.qido_url;
        let url = format!("{base_url}/instances");

        QidoRequest::new(self.clone(), url)
    }

    /// Create a QIDO-RS request to query all instances in a specific series
    pub fn query_instances_in_series(
        &self,
        study_instance_uid: &str,
        series_instance_uid: &str,
    ) -> QidoRequest {
        let base_url = &self.qido_url;
        let url = format!(
            "{base_url}/studies/{study_instance_uid}/series/{series_instance_uid}/instances",
        );

        QidoRequest::new(self.clone(), url)
    }
}
