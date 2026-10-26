# DICOMWEB-rs

[![continuous integration](https://github.com/mbits-imaging/dicomweb-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/mbits-imaging/dicomweb-rs/actions/workflows/rust.yml)

This is a DICOMweb client library, using the excellent [dicom-rs](https://github.com/Enet4/dicom-rs) crate.

## Features

- [x] QIDO-RS:
  - /studies: query all studies
  - /studies/{uid}/series: query all series inside a specific study
  - /studies/{uid}/series/{uid}/instances: query all instances in a specific series
  - /series: query all series
  - /instances: query all instances
- [x] WADO-RS:
  - /studies/{uid}: retrieve all instances in a specific study
  - /studies/{uid}/metadata: retrieve metadata for all instances in a study
  - /studies/{uid}/series/{uid}: retrieve all instances in a specific series
  - /studies/{uid}/series/{uid}/instances/{uid}: retrieve a single instance
  - /studies/{uid}/series/{uid}/instances/{uid}/metadata: retrieve metadata for a specific instance
  - /studies/{uid}/series/{uid}/instances/{uid}/frames/{framelist}: retrieve frame pixeldata for a specific instance
- [x] MPPS-RS:
  - POST /modality-performed-procedure-steps/{uid}: create a modality performed procedure step
  - POST /modality-performed-procedure-steps/{uid}?update: update a modality performed procedure step
  - GET /modality-performed-procedure-steps/{uid}: retrieve a modality performed procedure step
- [x] Storage Commitment:
  - POST /commitment-requests/{uid}: request storage commitment for a set of instances
  - GET /commitment-requests/{uid}: check the result of a storage commitment request

## Usage

```rust
use dicom_web::DicomWebClient;

let mut client = DicomWebClient::with_single_url("http://localhost:8042");
client.set_basic_auth("orthanc", "orthanc");

let studies = client.query_studies().run().await.unwrap();
```