# DICOMWEB-rs

This is a DICOMweb client library, using the excellent `dicom-rs` crate.

## Features

- [x] QIDO-RS:
  - /studies: query all studies
  - /studies/{uid}/series: query all series inside a specific study
  - /studies/{uid}/series/{uid}/instances: query all instances in a specific series
- [x] WADO-RS:
  - /studies/{uid}: retrieve all instances in a specific study
  - /studies/{uid}/series/{uid}: retrieve all instances in a specific series

## Usage

```rust
use dicom_web::DicomWebClient;

let mut client = DicomWebClient::with_single_url("http://localhost:8042");
client.set_basic_auth("orthanc", "orthanc");

let studies = client.query_studies().run().await.unwrap();
```